use std::sync::Arc;

use ezhpgl_core::{self as core, Payload};
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::PyBytes;

mod plot;

pyo3::create_exception!(_core, ParseError, PyValueError);

fn parse_error(py: Python<'_>, error: core::ParseError) -> PyErr {
    let exception = ParseError::new_err(error.to_string());
    let diagnostic = Py::new(py, PyDiagnostic::from(&error.diagnostic));
    match diagnostic.and_then(|d| exception.value(py).setattr("diagnostic", d)) {
        Ok(()) => exception,
        Err(attribute_error) => attribute_error,
    }
}

#[pyclass(
    name = "SourceSpan",
    frozen,
    get_all,
    skip_from_py_object,
    module = "ezhpgl._core"
)]
#[derive(Clone)]
struct PySpan {
    start: usize,
    end: usize,
}

impl From<core::SourceSpan> for PySpan {
    fn from(span: core::SourceSpan) -> Self {
        Self {
            start: span.start,
            end: span.end,
        }
    }
}

#[pymethods]
impl PySpan {
    fn __repr__(&self) -> String {
        format!("SourceSpan(start={}, end={})", self.start, self.end)
    }
}

#[pyclass(
    name = "Diagnostic",
    frozen,
    get_all,
    skip_from_py_object,
    module = "ezhpgl._core"
)]
#[derive(Clone)]
struct PyDiagnostic {
    code: String,
    message: String,
    span: PySpan,
    command: Option<String>,
    action: String,
}

impl From<&core::Diagnostic> for PyDiagnostic {
    fn from(d: &core::Diagnostic) -> Self {
        Self {
            code: d.code.into(),
            message: d.message.clone(),
            span: d.span.into(),
            command: d.command.clone(),
            action: d.action.into(),
        }
    }
}

#[pyclass(name = "Limits", frozen, skip_from_py_object, module = "ezhpgl._core")]
#[derive(Clone)]
struct PyLimits {
    inner: core::Limits,
}

#[pymethods]
impl PyLimits {
    #[new]
    #[pyo3(signature = (*, max_input_bytes=67_108_864, max_command_bytes=4_194_304, max_commands=1_000_000, max_arguments_per_command=262_144, max_diagnostics=1024))]
    fn new(
        max_input_bytes: usize,
        max_command_bytes: usize,
        max_commands: usize,
        max_arguments_per_command: usize,
        max_diagnostics: usize,
    ) -> Self {
        Self {
            inner: core::Limits {
                max_input_bytes,
                max_command_bytes,
                max_commands,
                max_arguments_per_command,
                max_diagnostics,
            },
        }
    }
    #[getter]
    fn max_input_bytes(&self) -> usize {
        self.inner.max_input_bytes
    }
    #[getter]
    fn max_command_bytes(&self) -> usize {
        self.inner.max_command_bytes
    }
    #[getter]
    fn max_commands(&self) -> usize {
        self.inner.max_commands
    }
    #[getter]
    fn max_arguments_per_command(&self) -> usize {
        self.inner.max_arguments_per_command
    }
    #[getter]
    fn max_diagnostics(&self) -> usize {
        self.inner.max_diagnostics
    }
}

#[pyclass(name = "DetectionResult", frozen, get_all, module = "ezhpgl._core")]
struct PyDetection {
    container: String,
    language: String,
    reasons: Vec<String>,
}

#[pyclass(name = "Command", frozen, module = "ezhpgl._core")]
struct PyCommand {
    document: Arc<core::RawDocument>,
    index: usize,
}

#[pymethods]
impl PyCommand {
    #[getter]
    fn name(&self) -> String {
        self.document.commands[self.index].name().into()
    }
    #[getter]
    fn kind(&self) -> &'static str {
        self.document.commands[self.index].kind()
    }
    #[getter]
    fn span(&self) -> PySpan {
        self.document.commands[self.index].span.into()
    }
    #[getter]
    fn payload_span(&self) -> PySpan {
        self.document.commands[self.index].payload_span.into()
    }
    #[getter]
    fn args(&self) -> Vec<f64> {
        match &self.document.commands[self.index].payload {
            Payload::Numbers(numbers) => numbers.iter().map(|n| n.value).collect(),
            _ => Vec::new(),
        }
    }
    #[getter]
    fn argument_spans(&self) -> Vec<PySpan> {
        match &self.document.commands[self.index].payload {
            Payload::Numbers(numbers) => numbers.iter().map(|n| n.span.into()).collect(),
            _ => Vec::new(),
        }
    }
    #[getter]
    fn raw<'py>(&self, py: Python<'py>) -> Bound<'py, PyBytes> {
        let span = self.document.commands[self.index].span;
        PyBytes::new(py, &self.document.source[span.start..span.end])
    }
    #[getter]
    fn data<'py>(&self, py: Python<'py>) -> Bound<'py, PyBytes> {
        let span = self.document.commands[self.index].payload_span;
        PyBytes::new(py, &self.document.source[span.start..span.end])
    }
    #[getter]
    fn terminator(&self) -> Option<u8> {
        match self.document.commands[self.index].payload {
            Payload::Label { terminator, .. } | Payload::LabelTerminator { terminator, .. } => {
                Some(terminator)
            }
            _ => None,
        }
    }
    #[getter]
    fn print_terminator(&self) -> Option<bool> {
        match self.document.commands[self.index].payload {
            Payload::Label {
                print_terminator, ..
            }
            | Payload::LabelTerminator {
                print_terminator, ..
            } => Some(print_terminator),
            _ => None,
        }
    }
    fn __repr__(&self) -> String {
        let command = &self.document.commands[self.index];
        format!(
            "Command(name={:?}, kind={:?}, span={}..{})",
            command.name(),
            command.kind(),
            command.span.start,
            command.span.end
        )
    }
}

#[pyclass(name = "RawDocument", frozen, module = "ezhpgl._core")]
struct PyDocument {
    inner: Arc<core::RawDocument>,
}

#[pymethods]
impl PyDocument {
    #[getter]
    fn completeness(&self) -> &'static str {
        if self.inner.complete {
            "complete"
        } else {
            "partial"
        }
    }
    #[getter]
    fn diagnostics(&self) -> Vec<PyDiagnostic> {
        self.inner.diagnostics.iter().map(Into::into).collect()
    }
    #[getter]
    fn commands(&self) -> Vec<PyCommand> {
        (0..self.inner.commands.len())
            .map(|index| PyCommand {
                document: self.inner.clone(),
                index,
            })
            .collect()
    }
    #[getter]
    fn source_size(&self) -> usize {
        self.inner.source.len()
    }
    fn __len__(&self) -> usize {
        self.inner.commands.len()
    }
    fn __iter__(&self) -> PyCommandIterator {
        PyCommandIterator {
            document: self.inner.clone(),
            index: 0,
        }
    }
}

#[pyclass(name = "CommandIterator", module = "ezhpgl._core")]
struct PyCommandIterator {
    document: Arc<core::RawDocument>,
    index: usize,
}

#[pymethods]
impl PyCommandIterator {
    fn __iter__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
        slf
    }
    fn __next__(&mut self) -> Option<PyCommand> {
        if self.index == self.document.commands.len() {
            return None;
        }
        let result = PyCommand {
            document: self.document.clone(),
            index: self.index,
        };
        self.index += 1;
        Some(result)
    }
}

#[pyfunction]
fn detect_format(data: &[u8]) -> PyDetection {
    let result = core::detect_format(data);
    PyDetection {
        container: result.container.into(),
        language: result.language.into(),
        reasons: result.reasons.into_iter().map(Into::into).collect(),
    }
}

#[pyfunction]
#[pyo3(signature = (data, *, strict=true, limits=None))]
fn parse_commands(
    py: Python<'_>,
    data: &[u8],
    strict: bool,
    limits: Option<&PyLimits>,
) -> PyResult<PyDocument> {
    let options = core::ParseOptions {
        strict,
        limits: limits.map_or_else(core::Limits::default, |l| l.inner),
    };
    // Enforce the byte limit before allocating our owned buffer or releasing Python.
    let result = if data.len() > options.limits.max_input_bytes {
        Err(core::ParseError {
            diagnostic: core::Diagnostic {
                code: "LIMIT_EXCEEDED",
                message: "Input byte limit exceeded".into(),
                span: core::SourceSpan {
                    start: 0,
                    end: data.len(),
                },
                command: None,
                action: "stop",
            },
        })
    } else {
        let owned: Arc<[u8]> = Arc::from(data);
        py.detach(move || core::parse_owned_commands(owned, options))
    };
    result
        .map(|document| PyDocument {
            inner: Arc::new(document),
        })
        .map_err(|error| parse_error(py, error))
}

#[pymodule]
fn _core(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add("__version__", core::VERSION)?;
    m.add("ParseError", m.py().get_type::<ParseError>())?;
    m.add_class::<PySpan>()?;
    m.add_class::<PyDiagnostic>()?;
    m.add_class::<PyLimits>()?;
    m.add_class::<PyDetection>()?;
    m.add_class::<PyCommand>()?;
    m.add_class::<PyDocument>()?;
    m.add_class::<PyCommandIterator>()?;
    m.add_function(wrap_pyfunction!(detect_format, m)?)?;
    m.add_function(wrap_pyfunction!(parse_commands, m)?)?;
    plot::register(m)?;
    Ok(())
}
