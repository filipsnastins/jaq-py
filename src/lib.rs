//! Python bindings for jaq, a jq clone written in Rust.

use jaq_core::{load, Compiler, Ctx, Filter, Native, RcIter};
use jaq_json::Val;
use pyo3::exceptions::PyException;
use pyo3::prelude::*;
use pyo3::types::PyDict;
use pythonize::{depythonize, pythonize};
use std::collections::BTreeMap;
use std::sync::Arc;

pyo3::create_exception!(
    jaq_py,
    JaqError,
    PyException,
    "Base exception for jaq-py errors."
);
pyo3::create_exception!(
    jaq_py,
    JaqParseError,
    JaqError,
    "Error parsing a jaq filter."
);
pyo3::create_exception!(
    jaq_py,
    JaqCompileError,
    JaqError,
    "Error compiling a jaq filter."
);
pyo3::create_exception!(
    jaq_py,
    JaqRuntimeError,
    JaqError,
    "Error executing a jaq filter."
);
pyo3::create_exception!(
    jaq_py,
    JaqJsonError,
    JaqError,
    "Error converting data to/from JSON."
);

fn format_errors<E: std::fmt::Debug>(errs: &[E]) -> String {
    errs.iter()
        .map(|e| format!("{e:?}"))
        .collect::<Vec<_>>()
        .join("; ")
}

/// Compile a jaq filter string into a reusable JaqProgram.
///
/// Args:
///     filter: A jq/jaq filter expression (e.g., ".foo", ".[] | select(.x > 1)").
///     args: Optional dict of variables to bind (e.g., {"a": 1} makes $a available).
///
/// Returns:
///     A compiled JaqProgram that can be executed with different inputs.
///
/// Raises:
///     JaqParseError: If the filter syntax is invalid.
///     JaqCompileError: If the filter fails to compile.
///     JaqJsonError: If args values cannot be converted to JSON.
#[pyfunction]
#[pyo3(signature = (filter, args=None))]
fn compile(filter: &str, args: Option<&Bound<'_, PyDict>>) -> PyResult<JaqProgram> {
    let (var_names, var_values) = extract_args(args)?;

    let loader = load::Loader::new(jaq_std::defs().chain(jaq_json::defs()));
    let arena = load::Arena::default();
    let modules = loader
        .load(
            &arena,
            load::File {
                code: filter,
                path: (),
            },
        )
        .map_err(|errs| {
            let errs: Vec<_> = errs.iter().map(|(_, e)| e).collect();
            JaqParseError::new_err(format_errors(&errs))
        })?;

    let var_names_prefixed: Vec<String> = var_names.iter().map(|v| format!("${v}")).collect();
    let compiled = Compiler::default()
        .with_funs(jaq_std::funs().chain(jaq_json::funs()))
        .with_global_vars(var_names_prefixed.iter().map(|s| s.as_str()))
        .compile(modules)
        .map_err(|errs| JaqCompileError::new_err(format_errors(&errs)))?;

    Ok(JaqProgram {
        filter: Arc::from(filter),
        compiled: Arc::new(compiled),
        var_names: Arc::new(var_names),
        var_values: Arc::new(var_values),
    })
}

fn extract_args(
    args: Option<&Bound<'_, PyDict>>,
) -> PyResult<(Vec<String>, BTreeMap<String, serde_json::Value>)> {
    let mut var_names = Vec::new();
    let mut var_values = BTreeMap::new();

    if let Some(args_dict) = args {
        var_names.reserve(args_dict.len());
        for (key, value) in args_dict.iter() {
            let name: String = key.extract()?;
            let json_value: serde_json::Value = depythonize(&value).map_err(|e| {
                JaqJsonError::new_err(format!("Failed to convert arg '{name}': {e}"))
            })?;
            var_names.push(name.clone());
            var_values.insert(name, json_value);
        }
    }

    Ok((var_names, var_values))
}

/// A compiled jaq program ready to accept input.
#[pyclass]
#[derive(Clone)]
struct JaqProgram {
    filter: Arc<str>,
    compiled: Arc<Filter<Native<Val>>>,
    var_names: Arc<Vec<String>>,
    var_values: Arc<BTreeMap<String, serde_json::Value>>,
}

impl JaqProgram {
    fn build_vars(&self) -> Vec<Val> {
        self.var_names
            .iter()
            .filter_map(|name| self.var_values.get(name))
            .map(|v| Val::from(v.clone()))
            .collect()
    }

    fn with_input(&self, input: serde_json::Value) -> JaqProgramWithInput {
        JaqProgramWithInput {
            compiled: Arc::clone(&self.compiled),
            filter: Arc::clone(&self.filter),
            input: input.into(),
            vars: self.build_vars(),
        }
    }
}

#[pymethods]
impl JaqProgram {
    /// The original filter string that was compiled.
    #[getter]
    fn program_string(&self) -> &str {
        &self.filter
    }

    /// Provide input as a raw JSON string (fast path, skips Python object traversal).
    ///
    /// Args:
    ///     text: A JSON-encoded string, or multiple whitespace-separated JSON values if slurp=True.
    ///     slurp: If True, read all JSON values into an array.
    ///
    /// Returns:
    ///     A JaqProgramWithInput that can be executed with `first()` or `all()`.
    ///
    /// Raises:
    ///     JaqJsonError: If the string is not valid JSON.
    #[pyo3(signature = (text, slurp=false))]
    fn input_text(&self, text: &str, slurp: bool) -> PyResult<JaqProgramWithInput> {
        let json_value = if slurp {
            let stream = serde_json::Deserializer::from_str(text).into_iter::<serde_json::Value>();
            let values: Result<Vec<_>, _> = stream
                .map(|r| r.map_err(|e| JaqJsonError::new_err(format!("Failed to parse JSON: {e}"))))
                .collect();
            serde_json::Value::Array(values?)
        } else {
            serde_json::from_str(text)
                .map_err(|e| JaqJsonError::new_err(format!("Failed to parse JSON: {e}")))?
        };
        Ok(self.with_input(json_value))
    }

    /// Provide input as a Python object (convenient, but slower for large data).
    ///
    /// Args:
    ///     value: Any JSON-serializable Python object.
    ///
    /// Returns:
    ///     A JaqProgramWithInput that can be executed with `first()` or `all()`.
    ///
    /// Raises:
    ///     JaqJsonError: If the value cannot be converted to JSON.
    fn input_value(&self, value: &Bound<'_, PyAny>) -> PyResult<JaqProgramWithInput> {
        let json_value: serde_json::Value = depythonize(value)
            .map_err(|e| JaqJsonError::new_err(format!("Failed to convert input: {e}")))?;
        Ok(self.with_input(json_value))
    }

    fn __repr__(&self) -> String {
        format!("JaqProgram({:?})", self.filter)
    }
}

/// A compiled jaq program with input, ready to execute.
// Note: unsendable because Val contains Rc which isn't Send.
#[pyclass(unsendable)]
struct JaqProgramWithInput {
    filter: Arc<str>,
    compiled: Arc<Filter<Native<Val>>>,
    input: Val,
    vars: Vec<Val>,
}

impl JaqProgramWithInput {
    fn run_first(&self) -> Option<Result<Val, jaq_core::Error<Val>>> {
        let inputs = RcIter::new(std::iter::empty());
        let ctx = Ctx::new(self.vars.iter().cloned(), &inputs);
        let result = self.compiled.run((ctx, self.input.clone())).next();
        result
    }

    fn run_all(&self) -> Vec<Result<Val, jaq_core::Error<Val>>> {
        let inputs = RcIter::new(std::iter::empty());
        let ctx = Ctx::new(self.vars.iter().cloned(), &inputs);
        let result = self.compiled.run((ctx, self.input.clone())).collect();
        result
    }
}

#[pymethods]
impl JaqProgramWithInput {
    /// The original filter string that was compiled.
    #[getter]
    fn program_string(&self) -> &str {
        &self.filter
    }

    /// Execute the filter and return the first result.
    ///
    /// Returns:
    ///     The first output value, or None if the filter produces no output.
    ///
    /// Raises:
    ///     JaqRuntimeError: If an error occurs during filter execution.
    fn first(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        match self.run_first() {
            Some(Ok(val)) => val_to_python(py, val),
            Some(Err(e)) => Err(JaqRuntimeError::new_err(format!("{e:?}"))),
            None => Ok(py.None()),
        }
    }

    /// Execute the filter and return the first result as a JSON string.
    ///
    /// Returns:
    ///     The first output value serialized as JSON, or None if no output.
    ///
    /// Raises:
    ///     JaqRuntimeError: If an error occurs during filter execution.
    fn text(&self) -> PyResult<Option<String>> {
        match self.run_first() {
            Some(Ok(val)) => Ok(Some(val.to_string())),
            Some(Err(e)) => Err(JaqRuntimeError::new_err(format!("{e:?}"))),
            None => Ok(None),
        }
    }

    /// Execute the filter and return all results as a list.
    ///
    /// Returns:
    ///     A list of all output values produced by the filter.
    ///
    /// Raises:
    ///     JaqRuntimeError: If an error occurs during filter execution.
    fn all(&self, py: Python<'_>) -> PyResult<Vec<Py<PyAny>>> {
        self.run_all()
            .into_iter()
            .map(|r| match r {
                Ok(val) => val_to_python(py, val),
                Err(e) => Err(JaqRuntimeError::new_err(format!("{e:?}"))),
            })
            .collect()
    }

    fn __repr__(&self) -> String {
        format!("JaqProgramWithInput({:?})", self.filter)
    }
}

fn val_to_python(py: Python<'_>, val: Val) -> PyResult<Py<PyAny>> {
    let json_value = serde_json::Value::from(val);
    pythonize(py, &json_value)
        .map(|bound| bound.unbind())
        .map_err(|e| JaqRuntimeError::new_err(format!("Failed to convert output: {e}")))
}

#[pymodule]
#[pyo3(name = "_jaq_py")]
fn jaq_py(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add("JaqError", m.py().get_type::<JaqError>())?;
    m.add("JaqParseError", m.py().get_type::<JaqParseError>())?;
    m.add("JaqCompileError", m.py().get_type::<JaqCompileError>())?;
    m.add("JaqRuntimeError", m.py().get_type::<JaqRuntimeError>())?;
    m.add("JaqJsonError", m.py().get_type::<JaqJsonError>())?;
    m.add_function(wrap_pyfunction!(compile, m)?)?;
    m.add_class::<JaqProgram>()?;
    m.add_class::<JaqProgramWithInput>()?;
    Ok(())
}
