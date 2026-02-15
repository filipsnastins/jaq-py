//! Python bindings for jaq, a jq clone written in Rust.

#[global_allocator]
static ALLOC: mimalloc::MiMalloc = mimalloc::MiMalloc;

use jaq_core::load::{Arena, File, Loader};
use jaq_core::{data, unwrap_valr, Compiler, Ctx, Native, Vars};
use jaq_json::{Map, Num, Tag, Val};
use num_bigint::BigInt;
use pyo3::exceptions::PyException;
use pyo3::prelude::*;
use pyo3::types::{PyBool, PyBytes, PyDict, PyFloat, PyInt, PyList, PyString, PyTuple};
use std::fmt::Write;
use std::rc::Rc;

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
    let mut buf = String::new();
    for (i, e) in errs.iter().enumerate() {
        if i > 0 {
            buf.push_str("; ");
        }
        write!(buf, "{e:?}").unwrap();
    }
    buf
}

fn runtime_err(e: jaq_core::Error<Val>) -> PyErr {
    JaqRuntimeError::new_err(format!("{e:?}"))
}

type CompiledFilter = jaq_core::compile::Filter<Native<data::JustLut<Val>>>;
type FilterResult = Result<Val, jaq_core::Error<Val>>;

/// Compile a jaq filter string into a reusable JaqProgram.
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
        filter: Rc::from(filter),
        compiled: Rc::new(compiled),
        vars: Rc::from(vars),
    })
}

fn extract_args(args: Option<&Bound<'_, PyDict>>) -> PyResult<(Vec<String>, Vec<Val>)> {
    let Some(args_dict) = args else {
        return Ok((Vec::new(), Vec::new()));
    };

    args_dict
        .iter()
        .map(|(key, value)| {
            let name: String = key.extract()?;
            let val = python_to_val(&value)?;
            Ok((name, val))
        })
        .collect::<PyResult<Vec<_>>>()
        .map(|pairs| pairs.into_iter().unzip())
}

/// A compiled jaq program, ready to accept input.
#[pyclass(frozen, unsendable)]
struct JaqProgram {
    filter: Rc<str>,
    compiled: Rc<CompiledFilter>,
    vars: Rc<[Val]>,
}

impl JaqProgram {
    fn with_input(&self, input: Val) -> JaqProgramWithInput {
        JaqProgramWithInput {
            compiled: Rc::clone(&self.compiled),
            filter: Rc::clone(&self.filter),
            input,
            vars: Rc::clone(&self.vars),
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

    /// Provide input as a raw JSON string (fast path — skips Python object traversal).
    #[pyo3(signature = (text, slurp=false))]
    fn input_text(&self, text: &str, slurp: bool) -> PyResult<JaqProgramWithInput> {
        let bytes = text.as_bytes();
        let input = if slurp {
            jaq_json::read::parse_many(bytes)
                .map(|r| r.map_err(|e| format!("Failed to parse JSON: {e}")))
                .collect::<Result<Vec<Val>, String>>()
                .map(|v| v.into_iter().collect::<Val>())
        } else {
            jaq_json::read::parse_single(bytes).map_err(|e| format!("Failed to parse JSON: {e}"))
        }
        .map_err(JaqJsonError::new_err)?;
        Ok(self.with_input(input))
    }

    /// Provide input as a Python object (convenient, but slower for large data).
    fn input_value(&self, value: &Bound<'_, PyAny>) -> PyResult<JaqProgramWithInput> {
        let val = python_to_val(value)?;
        Ok(self.with_input(val))
    }

    fn __repr__(&self) -> String {
        format!("JaqProgram({:?})", self.filter)
    }
}

/// A compiled jaq program with input bound, ready to execute.
#[pyclass(frozen, unsendable)]
struct JaqProgramWithInput {
    filter: Rc<str>,
    compiled: Rc<CompiledFilter>,
    input: Val,
    vars: Rc<[Val]>,
}

impl JaqProgramWithInput {
    /// Run the compiled filter, passing the result iterator to `f`.
    fn run<T>(&self, f: impl FnOnce(&mut dyn Iterator<Item = FilterResult>) -> T) -> T {
        let input = self.input.clone();
        let vars = Vars::new(self.vars.iter().cloned());
        let ctx = Ctx::<data::JustLut<Val>>::new(&self.compiled.lut, vars);
        let mut iter = self.compiled.id.run((ctx, input)).map(unwrap_valr);
        f(&mut iter)
    }
}

#[pymethods]
impl JaqProgramWithInput {
    /// The original filter string that was compiled.
    #[getter]
    fn program_string(&self) -> &str {
        &self.filter
    }

    /// Execute the filter and return the first result as a Python object.
    fn first(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        self.run(|iter| match iter.next() {
            Some(Ok(val)) => val_to_python(py, val),
            Some(Err(e)) => Err(runtime_err(e)),
            None => Ok(py.None()),
        })
    }

    /// Execute the filter and return the first result as a JSON string.
    fn first_text(&self) -> PyResult<Option<String>> {
        self.run(|iter| match iter.next() {
            Some(Ok(val)) => Ok(Some(val.to_string())),
            Some(Err(e)) => Err(runtime_err(e)),
            None => Ok(None),
        })
    }

    /// Execute the filter and return all results as a list of Python objects.
    fn all(&self, py: Python<'_>) -> PyResult<Vec<Py<PyAny>>> {
        self.run(|iter| {
            iter.map(|result| match result {
                Ok(val) => val_to_python(py, val),
                Err(e) => Err(runtime_err(e)),
            })
            .collect()
        })
    }

    /// Execute the filter and return all results as a newline-separated JSON string (JSONL).
    ///
    /// Fastest output path — skips Python object creation entirely.
    fn all_text(&self) -> PyResult<String> {
        self.run(|iter| {
            let mut buf = String::new();
            for result in iter {
                match result {
                    Ok(val) => {
                        if !buf.is_empty() {
                            buf.push('\n');
                        }
                        write!(buf, "{val}").unwrap();
                    }
                    Err(e) => return Err(runtime_err(e)),
                }
            }
            Ok(buf)
        })
    }

    fn __repr__(&self) -> String {
        format!("JaqProgramWithInput({:?})", self.filter)
    }
}

/// Convert a jaq Val to a Python object, bypassing serde_json.
fn val_to_python(py: Python<'_>, val: Val) -> PyResult<Py<PyAny>> {
    match val {
        Val::Null => Ok(py.None()),
        Val::Bool(b) => Ok(b.into_pyobject(py)?.to_owned().into_any().unbind()),
        Val::Num(n) => num_to_python(py, n),
        Val::Str(bytes, Tag::Utf8) => {
            // SAFETY: jaq guarantees that Tag::Utf8 strings contain valid UTF-8.
            let s = unsafe { std::str::from_utf8_unchecked(&bytes) };
            Ok(PyString::new(py, s).into_any().unbind())
        }
        Val::Str(bytes, Tag::Bytes) => Ok(PyBytes::new(py, &bytes).into_any().unbind()),
        Val::Arr(a) => {
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

fn num_to_python(py: Python<'_>, num: Num) -> PyResult<Py<PyAny>> {
    match num {
        Num::Int(i) => Ok(i.into_pyobject(py)?.into_any().unbind()),
        Num::BigInt(bi) => Ok(bi.as_ref().into_pyobject(py)?.into_any().unbind()),
        Num::Float(f) => Ok(f.into_pyobject(py)?.into_any().unbind()),
        Num::Dec(s) => {
            let f: f64 = s.parse().map_err(|_| {
                JaqJsonError::new_err(format!("Cannot represent decimal '{s}' as float"))
            })?;
            Ok(f.into_pyobject(py)?.into_any().unbind())
        }
    }
}

/// Convert a Python object to a jaq Val, bypassing serde_json.
fn python_to_val(obj: &Bound<'_, PyAny>) -> PyResult<Val> {
    // Bool must be checked before int (bool is a subclass of int in Python).
    if obj.is_none() {
        Ok(Val::Null)
    } else if let Ok(b) = obj.cast::<PyBool>() {
        Ok(Val::Bool(b.is_true()))
    } else if obj.is_instance_of::<PyInt>() {
        // Uses is_instance_of (not cast) because we try the fast isize extraction
        // first, falling back to BigInt only on overflow.
        if let Ok(i) = obj.extract::<isize>() {
            Ok(Val::Num(Num::Int(i)))
        } else {
            let bi: BigInt = obj.extract()?;
            Ok(Val::Num(Num::big_int(bi)))
        }
    } else if let Ok(f) = obj.cast::<PyFloat>() {
        Ok(Val::Num(Num::Float(f.value())))
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
        Err(JaqJsonError::new_err(format!(
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
