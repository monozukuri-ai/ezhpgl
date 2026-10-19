use super::PyPoint;
use crate::PySpan;
use ezhpgl_core as core;
use pyo3::{prelude::*, types::PyBytes};
use std::sync::Arc;

#[pyclass(
    name = "FontSpec",
    frozen,
    get_all,
    skip_from_py_object,
    module = "ezhpgl._core"
)]
#[derive(Clone)]
pub(super) struct PyFontSpec {
    symbol_set: u16,
    spacing: u8,
    pitch: f64,
    height_points: f64,
    posture: u16,
    weight: i16,
    typeface: u16,
}
impl From<core::FontSpec> for PyFontSpec {
    fn from(v: core::FontSpec) -> Self {
        Self {
            symbol_set: v.symbol_set,
            spacing: v.spacing,
            pitch: v.pitch,
            height_points: v.height_points,
            posture: v.posture,
            weight: v.weight,
            typeface: v.typeface,
        }
    }
}

#[pyclass(
    name = "TextState",
    frozen,
    get_all,
    skip_from_py_object,
    module = "ezhpgl._core"
)]
#[derive(Clone)]
pub(super) struct PyTextState {
    standard: PyFontSpec,
    alternate: PyFontSpec,
    use_alternate: bool,
    size_mode: &'static str,
    width: f64,
    height: f64,
    direction: PyPoint,
    relative_direction: bool,
    origin: u8,
    slant: f64,
    extra_space: PyPoint,
    path: u8,
    line: u8,
    transparent: bool,
    carriage_return: PyPoint,
}
impl From<core::TextState> for PyTextState {
    fn from(v: core::TextState) -> Self {
        Self {
            standard: v.standard.into(),
            alternate: v.alternate.into(),
            use_alternate: v.use_alternate,
            size_mode: v.size_mode,
            width: v.width,
            height: v.height,
            direction: v.direction.into(),
            relative_direction: v.relative_direction,
            origin: v.origin,
            slant: v.slant,
            extra_space: v.extra_space.into(),
            path: v.path,
            line: v.line,
            transparent: v.transparent,
            carriage_return: v.carriage_return.into(),
        }
    }
}

#[pyclass(name = "Glyph", frozen, get_all, module = "ezhpgl._core")]
pub(super) struct PyGlyph {
    character: String,
    source_offset: usize,
    origin: PyPoint,
    axis_x: PyPoint,
    axis_y: PyPoint,
    font: PyFontSpec,
    envelope: Vec<PyPoint>,
}
impl From<core::Glyph> for PyGlyph {
    fn from(v: core::Glyph) -> Self {
        Self {
            character: v.character.to_string(),
            source_offset: v.source_offset,
            origin: v.origin.into(),
            axis_x: v.axis_x.into(),
            axis_y: v.axis_y.into(),
            font: v.font.into(),
            envelope: v.envelope.into_iter().map(Into::into).collect(),
        }
    }
}

#[pyclass(name = "InputEvent", frozen, get_all, module = "ezhpgl._core")]
pub(super) struct PyInputEvent {
    kind: &'static str,
    span: PySpan,
    command: String,
    payload_span: Option<PySpan>,
}
impl From<core::InputEvent> for PyInputEvent {
    fn from(v: core::InputEvent) -> Self {
        Self {
            kind: v.kind,
            span: v.span.into(),
            command: v.command,
            payload_span: v.payload_span.map(Into::into),
        }
    }
}

#[pyclass(name = "Label", frozen, module = "ezhpgl._core")]
pub(super) struct PyLabel {
    pub(super) document: Arc<core::PlotDocument>,
    pub(super) page: usize,
    pub(super) operation: usize,
}
impl PyLabel {
    fn inner(&self) -> &core::Label {
        match &self.document.pages[self.page].operations[self.operation].geometry {
            core::Geometry::Label { label } => label,
            _ => unreachable!("Label wrappers only reference label operations"),
        }
    }
}
#[pymethods]
impl PyLabel {
    #[getter]
    fn data<'py>(&self, py: Python<'py>) -> Bound<'py, PyBytes> {
        PyBytes::new(py, &self.inner().data)
    }
    #[getter]
    fn payload_span(&self) -> PySpan {
        self.inner().payload_span.into()
    }
    #[getter]
    fn terminator(&self) -> u8 {
        self.inner().terminator
    }
    #[getter]
    fn print_terminator(&self) -> bool {
        self.inner().print_terminator
    }
    #[getter]
    fn decoded(&self) -> String {
        self.inner().decoded.clone()
    }
    #[getter]
    fn glyphs(&self) -> Vec<PyGlyph> {
        self.inner()
            .glyphs
            .iter()
            .cloned()
            .map(Into::into)
            .collect()
    }
    #[getter]
    fn unmapped_offsets(&self) -> Vec<usize> {
        self.inner().unmapped_offsets.clone()
    }
    #[getter]
    fn state(&self) -> PyTextState {
        self.inner().state.clone().into()
    }
    #[getter]
    fn start(&self) -> PyPoint {
        self.inner().start.into()
    }
    #[getter]
    fn end(&self) -> PyPoint {
        self.inner().end.into()
    }
    #[getter]
    fn approximate(&self) -> bool {
        self.inner().approximate
    }
}
pub(super) fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyFontSpec>()?;
    m.add_class::<PyTextState>()?;
    m.add_class::<PyGlyph>()?;
    m.add_class::<PyLabel>()?;
    m.add_class::<PyInputEvent>()?;
    Ok(())
}
