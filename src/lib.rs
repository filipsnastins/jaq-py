//! Python bindings for jaq, a jq clone written in Rust.

use jaq_core::load::{Arena, File, Loader};
use jaq_core::{data, unwrap_valr, Compiler, Ctx, Native, Vars};
use jaq_json::{Map, Num, Tag, Val};
use num_bigint::BigInt;
use pyo3::exceptions::{PyException, PyTypeError};
use pyo3::prelude::*;
use pyo3::types::{PyBool, PyBytes, PyDict, PyFloat, PyInt, PyList, PyString, PyTuple};
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

/// The compiled filter type: JustLut for simple value processing, wrapped in Native.
type CompiledFilter = jaq_core::compile::Filter<Native<data::JustLut<Val>>>;

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
    let (var_names, vars) = extract_args(args)?;

    let loader = Loader::new(jaq_std::defs().chain(jaq_json::defs()));
    let arena = Arena::default();
    let modules = loader
        .load(
            &arena,
            File {
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
        vars: Arc::new(vars),
    })
}

fn extract_args(args: Option<&Bound<'_, PyDict>>) -> PyResult<(Vec<String>, Vec<Val>)> {
    let mut var_names = Vec::new();
    let mut var_values = Vec::new();

    if let Some(args_dict) = args {
        var_names.reserve(args_dict.len());
        var_values.reserve(args_dict.len());
        for (key, value) in args_dict.iter() {
            let name: String = key.extract()?;
            let val = python_to_val(&value).map_err(|e| {
                JaqJsonError::new_err(format!("Failed to convert arg '{name}': {e}"))
            })?;
            var_names.push(name);
            var_values.push(val);
        }
    }

    Ok((var_names, var_values))
}

/// A compiled jaq program ready to accept input.
#[pyclass(frozen, skip_from_py_object)]
#[derive(Clone)]
struct JaqProgram {
    filter: Arc<str>,
    compiled: Arc<CompiledFilter>,
    vars: Arc<Vec<Val>>,
}

impl JaqProgram {
    fn with_input(&self, input: Val) -> JaqProgramWithInput {
        JaqProgramWithInput {
            compiled: Arc::clone(&self.compiled),
            filter: Arc::clone(&self.filter),
            input,
            vars: (*self.vars).clone(),
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

    /// Provide input as a raw JSON string (fast path, parses directly to jaq values).
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
        let input = if slurp {
            let values: Result<Vec<Val>, _> = jaq_json::read::parse_many(text.as_bytes())
                .map(|r| r.map_err(|e| JaqJsonError::new_err(format!("Failed to parse JSON: {e}"))))
                .collect();
            values?.into_iter().collect::<Val>()
        } else {
            jaq_json::read::parse_single(text.as_bytes())
                .map_err(|e| JaqJsonError::new_err(format!("Failed to parse JSON: {e}")))?
        };
        Ok(self.with_input(input))
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
        let val = python_to_val(value)
            .map_err(|e| JaqJsonError::new_err(format!("Failed to convert input: {e}")))?;
        Ok(self.with_input(val))
    }

    fn __repr__(&self) -> String {
        format!("JaqProgram({:?})", self.filter)
    }
}

/// A compiled jaq program with input, ready to execute.
// Note: unsendable because the run iterator uses Rc internally.
#[pyclass(unsendable)]
struct JaqProgramWithInput {
    filter: Arc<str>,
    compiled: Arc<CompiledFilter>,
    input: Val,
    vars: Vec<Val>,
}

impl JaqProgramWithInput {
    /// Run the filter and return a lazy iterator of results.
    fn run(&self) -> impl Iterator<Item = Result<Val, jaq_core::Error<Val>>> + use<'_> {
        let vars = Vars::new(self.vars.iter().cloned());
        let ctx = Ctx::<data::JustLut<Val>>::new(&self.compiled.lut, vars);
        self.compiled
            .id
            .run((ctx, self.input.clone()))
            .map(unwrap_valr)
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
        match self.run().next() {
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
    fn first_text(&self) -> PyResult<Option<String>> {
        match self.run().next() {
            Some(Ok(val)) => Ok(Some(val.to_string())),
            Some(Err(e)) => Err(JaqRuntimeError::new_err(format!("{e:?}"))),
            None => Ok(None),
        }
    }

    /// Execute the filter and return all results as a list.
    ///
    /// Each Val is converted to a Python object and immediately released,
    /// so the full Val result set is never held in memory at once.
    ///
    /// Returns:
    ///     A list of all output values produced by the filter.
    ///
    /// Raises:
    ///     JaqRuntimeError: If an error occurs during filter execution.
    fn all(&self, py: Python<'_>) -> PyResult<Vec<Py<PyAny>>> {
        self.run()
            .map(|r| match r {
                Ok(val) => val_to_python(py, val),
                Err(e) => Err(JaqRuntimeError::new_err(format!("{e:?}"))),
            })
            .collect()
    }

    /// Execute the filter and return all results as JSON strings.
    ///
    /// This is the fastest output path — it skips Python object creation entirely
    /// and serializes each output value directly to a JSON string.
    ///
    /// Returns:
    ///     A list of JSON-encoded strings, one per output value.
    ///
    /// Raises:
    ///     JaqRuntimeError: If an error occurs during filter execution.
    fn all_text(&self) -> PyResult<Vec<String>> {
        self.run()
            .map(|r| match r {
                Ok(val) => Ok(val.to_string()),
                Err(e) => Err(JaqRuntimeError::new_err(format!("{e:?}"))),
            })
            .collect()
    }

    fn __repr__(&self) -> String {
        format!("JaqProgramWithInput({:?})", self.filter)
    }
}

/// Convert a jaq Val directly to a Python object.
///
/// This avoids the intermediate serde_json::Value conversion,
/// and preserves byte strings (Tag::Bytes) as Python bytes.
fn val_to_python(py: Python<'_>, val: Val) -> PyResult<Py<PyAny>> {
    match val {
        Val::Null => Ok(py.None()),
        Val::Bool(b) => Ok(b.into_pyobject(py)?.to_owned().into_any().unbind()),
        Val::Num(n) => num_to_python(py, n),
        Val::Str(bytes, Tag::Utf8) => {
            // Fast path: most JSON strings are valid UTF-8, avoid lossy allocation
            let s = match std::str::from_utf8(&bytes) {
                Ok(s) => s,
                Err(_) => return Ok(PyBytes::new(py, &bytes).into_any().unbind()),
            };
            Ok(PyString::new(py, s).into_any().unbind())
        }
        Val::Str(bytes, Tag::Bytes) => Ok(PyBytes::new(py, &bytes).into_any().unbind()),
        Val::Arr(a) => {
            // Pre-convert all items, then build the list in one shot
            let items: Vec<Py<PyAny>> = a
                .iter()
                .map(|item| val_to_python(py, item.clone()))
                .collect::<PyResult<_>>()?;
            Ok(PyList::new(py, &items)?.into_any().unbind())
        }
        Val::Obj(o) => {
            let dict = PyDict::new(py);
            for (k, v) in o.iter() {
                let py_key = val_to_python(py, k.clone())?;
                let py_val = val_to_python(py, v.clone())?;
                dict.set_item(py_key, py_val)?;
            }
            Ok(dict.into_any().unbind())
        }
    }
}

/// Convert a jaq Num to a Python int or float.
fn num_to_python(py: Python<'_>, num: Num) -> PyResult<Py<PyAny>> {
    match num {
        Num::Int(i) => Ok(i.into_pyobject(py)?.into_any().unbind()),
        Num::BigInt(bi) => Ok((&*bi).into_pyobject(py)?.into_any().unbind()),
        Num::Float(f) => Ok(f.into_pyobject(py)?.into_any().unbind()),
        Num::Dec(s) => {
            // Decimal number string (e.g., "1.5", "1e10") -> Python float
            let f: f64 = s.parse().unwrap_or(f64::NAN);
            Ok(f.into_pyobject(py)?.into_any().unbind())
        }
    }
}

/// Convert a Python object directly to a jaq Val.
///
/// This avoids the intermediate serde_json::Value conversion,
/// and supports Python bytes -> Val byte strings.
fn python_to_val(obj: &Bound<'_, PyAny>) -> PyResult<Val> {
    // Order matters: check bool before int (bool is a subclass of int in Python)
    if obj.is_none() {
        Ok(Val::Null)
    } else if let Ok(b) = obj.cast::<PyBool>() {
        Ok(Val::Bool(b.is_true()))
    } else if obj.is_instance_of::<PyInt>() {
        // Try isize first for efficiency, fall back to BigInt for large values
        if let Ok(i) = obj.extract::<isize>() {
            Ok(Val::Num(Num::Int(i)))
        } else {
            let bi: BigInt = obj.extract()?;
            Ok(Val::Num(Num::big_int(bi)))
        }
    } else if obj.is_instance_of::<PyFloat>() {
        let f: f64 = obj.extract()?;
        Ok(Val::Num(Num::Float(f)))
    } else if let Ok(s) = obj.cast::<PyString>() {
        let s: String = s.extract()?;
        Ok(Val::utf8_str(s))
    } else if let Ok(b) = obj.cast::<PyBytes>() {
        Ok(Val::byte_str(b.as_bytes().to_vec()))
    } else if let Ok(list) = obj.cast::<PyList>() {
        let items: Vec<Val> = list
            .iter()
            .map(|item| python_to_val(&item))
            .collect::<PyResult<_>>()?;
        Ok(items.into_iter().collect())
    } else if let Ok(tuple) = obj.cast::<PyTuple>() {
        let items: Vec<Val> = tuple
            .iter()
            .map(|item| python_to_val(&item))
            .collect::<PyResult<_>>()?;
        Ok(items.into_iter().collect())
    } else if let Ok(dict) = obj.cast::<PyDict>() {
        let mut map = Map::default();
        for (k, v) in dict.iter() {
            map.insert(python_to_val(&k)?, python_to_val(&v)?);
        }
        Ok(Val::obj(map))
    } else {
        Err(PyTypeError::new_err(format!(
            "Cannot convert {} to jaq value",
            obj.get_type().name()?
        )))
    }
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
