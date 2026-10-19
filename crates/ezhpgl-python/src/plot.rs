use std::{collections::BTreeMap, path::PathBuf, sync::Arc};

use ezhpgl_core as core;
use pyo3::{exceptions::PyValueError, prelude::*};

use crate::{parse_error, PyDiagnostic, PyDocument, PyLimits, PySpan};

mod labels;
use labels::{PyInputEvent, PyLabel, PyTextState};

#[pyclass(
    name = "Point",
    frozen,
    get_all,
    skip_from_py_object,
    module = "ezhpgl._core"
)]
#[derive(Clone)]
pub(super) struct PyPoint {
    x: f64,
    y: f64,
}
impl From<core::Point> for PyPoint {
    fn from(p: core::Point) -> Self {
        Self { x: p.x, y: p.y }
    }
}

#[pyclass(
    name = "Bounds",
    frozen,
    get_all,
    skip_from_py_object,
    module = "ezhpgl._core"
)]
#[derive(Clone)]
struct PyBounds {
    min: PyPoint,
    max: PyPoint,
}
impl From<core::Bounds> for PyBounds {
    fn from(b: core::Bounds) -> Self {
        Self {
            min: b.min.into(),
            max: b.max.into(),
        }
    }
}

#[pyclass(name = "Style", frozen, get_all, module = "ezhpgl._core")]
struct PyStyle {
    pen: u32,
    color: String,
    width_mm: f64,
    line: Option<PyLineStyle>,
}

#[pyclass(
    name = "LineStyle",
    frozen,
    get_all,
    skip_from_py_object,
    module = "ezhpgl._core"
)]
#[derive(Clone)]
struct PyLineStyle {
    line_type: Option<i8>,
    pattern_length_mm: Option<f64>,
    phase_mm: f64,
    cap: u8,
    join: u8,
    miter_limit: f64,
}
impl From<core::LineStyle> for PyLineStyle {
    fn from(l: core::LineStyle) -> Self {
        Self {
            line_type: l.line_type,
            pattern_length_mm: l.pattern_length_mm,
            phase_mm: l.phase_mm,
            cap: l.cap,
            join: l.join,
            miter_limit: l.miter_limit,
        }
    }
}

#[pyclass(
    name = "Curve",
    frozen,
    get_all,
    skip_from_py_object,
    module = "ezhpgl._core"
)]
#[derive(Clone)]
struct PyCurve {
    kind: &'static str,
    center: Option<PyPoint>,
    axis_u: Option<PyPoint>,
    axis_v: Option<PyPoint>,
    start_angle_deg: Option<f64>,
    sweep_angle_deg: Option<f64>,
    chord_mode: Option<u8>,
    chord_value: Option<f64>,
    control_points: Vec<PyPoint>,
    tolerance_mm: Option<f64>,
}
impl From<core::Curve> for PyCurve {
    fn from(c: core::Curve) -> Self {
        let mut p = Self {
            kind: "",
            center: None,
            axis_u: None,
            axis_v: None,
            start_angle_deg: None,
            sweep_angle_deg: None,
            chord_mode: None,
            chord_value: None,
            control_points: Vec::new(),
            tolerance_mm: None,
        };
        match c {
            core::Curve::EllipticArc {
                center,
                axis_u,
                axis_v,
                start_angle_deg,
                sweep_angle_deg,
                chord,
            } => {
                p.kind = "elliptic_arc";
                p.center = Some(center.into());
                p.axis_u = Some(axis_u.into());
                p.axis_v = Some(axis_v.into());
                p.start_angle_deg = Some(start_angle_deg);
                p.sweep_angle_deg = Some(sweep_angle_deg);
                p.chord_mode = Some(chord.mode);
                p.chord_value = Some(chord.value);
            }
            core::Curve::CubicBezier {
                control_points,
                tolerance_mm,
            } => {
                p.kind = "cubic_bezier";
                p.control_points = control_points.into_iter().map(Into::into).collect();
                p.tolerance_mm = Some(tolerance_mm);
            }
        }
        p
    }
}

#[pyclass(name = "ContourEdge", frozen, get_all, module = "ezhpgl._core")]
struct PyContourEdge {
    start: usize,
    end: usize,
    pen_down: bool,
    source: PyProvenance,
    curve: Option<PyCurve>,
}

#[pyclass(name = "Contour", frozen, module = "ezhpgl._core")]
struct PyContour {
    contours: Arc<Vec<core::Contour>>,
    index: usize,
}
#[pymethods]
impl PyContour {
    #[getter]
    fn points(&self) -> Vec<PyPoint> {
        self.contours[self.index]
            .points
            .iter()
            .copied()
            .map(Into::into)
            .collect()
    }
    #[getter]
    fn edges(&self) -> Vec<PyContourEdge> {
        self.contours[self.index]
            .edges
            .iter()
            .map(|e| PyContourEdge {
                start: e.start,
                end: e.end,
                pen_down: e.pen_down,
                source: PyProvenance {
                    command_index: e.source.command_index,
                    command: e.source.command.clone(),
                    span: e.source.span.into(),
                },
                curve: e.curve.clone().map(Into::into),
            })
            .collect()
    }
}

#[pyclass(name = "FillStyle", frozen, get_all, module = "ezhpgl._core")]
struct PyFillStyle {
    fill_type: u8,
    rule: &'static str,
    spacing_mm: Option<f64>,
    angle_deg: f64,
    hatch_lines: Vec<(PyPoint, PyPoint)>,
    hatch_phases_mm: Vec<f64>,
}

#[pyclass(
    name = "Provenance",
    frozen,
    get_all,
    skip_from_py_object,
    module = "ezhpgl._core"
)]
#[derive(Clone)]
struct PyProvenance {
    command_index: usize,
    command: String,
    span: PySpan,
}

#[pyclass(name = "PlotState", frozen, get_all, module = "ezhpgl._core")]
struct PyPlotState {
    position: PyPoint,
    coordinate_mode: String,
    pen: u32,
    pen_down: bool,
    transform: Option<PyTransformState>,
    polygon_mode: bool,
    text: Option<PyTextState>,
    position_approximate: bool,
}

#[pyclass(name = "DeviceProfile", frozen, module = "ezhpgl._core")]
struct PyDeviceProfile {
    inner: core::DeviceProfile,
}

#[pymethods]
impl PyDeviceProfile {
    #[new]
    #[pyo3(signature = (*, plot_size_mm=None, paper_size_mm=None))]
    fn new(plot_size_mm: Option<(f64, f64)>, paper_size_mm: Option<(f64, f64)>) -> Self {
        Self {
            inner: core::DeviceProfile {
                plot_size_mm: plot_size_mm.map(|(w, h)| [w, h]),
                paper_size_mm: paper_size_mm.map(|(w, h)| [w, h]),
            },
        }
    }
    #[getter]
    fn plot_size_mm(&self) -> Option<(f64, f64)> {
        self.inner.plot_size_mm.map(|s| (s[0], s[1]))
    }
    #[getter]
    fn paper_size_mm(&self) -> Option<(f64, f64)> {
        self.inner.paper_size_mm.map(|s| (s[0], s[1]))
    }
}

#[pyclass(
    name = "ClipRegion",
    frozen,
    get_all,
    skip_from_py_object,
    module = "ezhpgl._core"
)]
#[derive(Clone)]
struct PyClipRegion {
    kind: &'static str,
    bounds: Option<PyBounds>,
}
impl From<core::ClipRegion> for PyClipRegion {
    fn from(c: core::ClipRegion) -> Self {
        Self {
            kind: match c {
                core::ClipRegion::Empty => "empty",
                _ => "rect",
            },
            bounds: c.bounds().map(Into::into),
        }
    }
}

#[pyclass(
    name = "TransformState",
    frozen,
    get_all,
    skip_from_py_object,
    module = "ezhpgl._core"
)]
#[derive(Clone)]
struct PyTransformState {
    p1_plotter_units: PyPoint,
    p2_plotter_units: Option<PyPoint>,
    rotation: u16,
    scaling: &'static str,
    matrix: [f64; 6],
    clip: Option<PyClipRegion>,
}
impl From<core::TransformState> for PyTransformState {
    fn from(s: core::TransformState) -> Self {
        Self {
            p1_plotter_units: s.p1_plotter_units.into(),
            p2_plotter_units: s.p2_plotter_units.map(Into::into),
            rotation: s.rotation,
            scaling: s.scaling,
            matrix: s.matrix,
            clip: s.clip.map(Into::into),
        }
    }
}

fn mode_name(mode: core::CoordinateMode) -> &'static str {
    match mode {
        core::CoordinateMode::Absolute => "absolute",
        core::CoordinateMode::Relative => "relative",
    }
}

#[pyclass(name = "GeometryLimits", frozen, module = "ezhpgl._core")]
struct PyGeometryLimits {
    inner: core::GeometryLimits,
}

#[pymethods]
impl PyGeometryLimits {
    #[new]
    #[pyo3(signature = (*, max_pages=10_000, max_operations=1_000_000, max_vertices=2_000_000, max_curve_segments=65_536, max_polygon_vertices=1_000_000, max_polygon_contours=10_000, max_hatch_lines=100_000, max_label_bytes=65_536, max_glyphs=200_000))]
    #[allow(clippy::too_many_arguments)]
    fn new(
        max_pages: usize,
        max_operations: usize,
        max_vertices: usize,
        max_curve_segments: usize,
        max_polygon_vertices: usize,
        max_polygon_contours: usize,
        max_hatch_lines: usize,
        max_label_bytes: usize,
        max_glyphs: usize,
    ) -> Self {
        Self {
            inner: core::GeometryLimits {
                max_pages,
                max_operations,
                max_vertices,
                max_curve_segments,
                max_polygon_vertices,
                max_polygon_contours,
                max_hatch_lines,
                max_label_bytes,
                max_glyphs,
            },
        }
    }
    #[getter]
    fn max_pages(&self) -> usize {
        self.inner.max_pages
    }
    #[getter]
    fn max_operations(&self) -> usize {
        self.inner.max_operations
    }
    #[getter]
    fn max_vertices(&self) -> usize {
        self.inner.max_vertices
    }
    #[getter]
    fn max_curve_segments(&self) -> usize {
        self.inner.max_curve_segments
    }
    #[getter]
    fn max_polygon_vertices(&self) -> usize {
        self.inner.max_polygon_vertices
    }
    #[getter]
    fn max_polygon_contours(&self) -> usize {
        self.inner.max_polygon_contours
    }
    #[getter]
    fn max_hatch_lines(&self) -> usize {
        self.inner.max_hatch_lines
    }
    #[getter]
    fn max_label_bytes(&self) -> usize {
        self.inner.max_label_bytes
    }
    #[getter]
    fn max_glyphs(&self) -> usize {
        self.inner.max_glyphs
    }
}

#[pyclass(name = "Geometry", frozen, module = "ezhpgl._core")]
struct PyGeometry {
    document: Arc<core::PlotDocument>,
    page: usize,
    operation: usize,
}

#[pymethods]
impl PyGeometry {
    #[getter]
    fn kind(&self) -> &'static str {
        match &self.document.pages[self.page].operations[self.operation].geometry {
            core::Geometry::Polyline { .. } => "polyline",
            core::Geometry::Curve { .. } => "curve",
            core::Geometry::Polygon { .. } => "polygon",
            core::Geometry::Label { .. } => "label",
        }
    }
    #[getter]
    fn points(&self) -> Vec<PyPoint> {
        self.document.pages[self.page].operations[self.operation]
            .geometry
            .points()
            .iter()
            .copied()
            .map(Into::into)
            .collect()
    }
    #[getter]
    fn label(&self) -> Option<PyLabel> {
        matches!(
            self.document.pages[self.page].operations[self.operation].geometry,
            core::Geometry::Label { .. }
        )
        .then(|| PyLabel {
            document: self.document.clone(),
            page: self.page,
            operation: self.operation,
        })
    }
    #[getter]
    fn start_dot(&self) -> bool {
        self.document.pages[self.page].operations[self.operation]
            .geometry
            .start_dot()
    }
    #[getter]
    fn curve(&self) -> Option<PyCurve> {
        self.document.pages[self.page].operations[self.operation]
            .geometry
            .curve()
            .cloned()
            .map(Into::into)
    }
    #[getter]
    fn contours(&self) -> Vec<PyContour> {
        match &self.document.pages[self.page].operations[self.operation].geometry {
            core::Geometry::Polygon { contours, .. } => (0..contours.len())
                .map(|index| PyContour {
                    contours: contours.clone(),
                    index,
                })
                .collect(),
            _ => Vec::new(),
        }
    }
    #[getter]
    fn fill(&self) -> Option<PyFillStyle> {
        match &self.document.pages[self.page].operations[self.operation].geometry {
            core::Geometry::Polygon { fill: Some(f), .. } => Some(PyFillStyle {
                fill_type: f.fill_type,
                rule: f.rule,
                spacing_mm: f.spacing_mm,
                angle_deg: f.angle_deg,
                hatch_phases_mm: f.hatch_phases_mm.clone(),
                hatch_lines: f
                    .hatch_lines
                    .iter()
                    .map(|v| (v[0].into(), v[1].into()))
                    .collect(),
            }),
            _ => None,
        }
    }
}

#[pyclass(name = "PlotOperation", frozen, module = "ezhpgl._core")]
struct PyOperation {
    document: Arc<core::PlotDocument>,
    page: usize,
    operation: usize,
}

#[pymethods]
impl PyOperation {
    #[getter]
    fn approximate(&self) -> bool {
        self.document.pages[self.page].operations[self.operation].approximate
    }
    #[getter]
    fn clip(&self) -> Option<PyClipRegion> {
        self.document.pages[self.page].operations[self.operation]
            .clip
            .map(Into::into)
    }
    #[getter]
    fn visible_bounds(&self) -> Option<PyBounds> {
        self.document.pages[self.page].operations[self.operation]
            .visible_bounds()
            .map(Into::into)
    }
    #[getter]
    fn geometry(&self) -> PyGeometry {
        PyGeometry {
            document: self.document.clone(),
            page: self.page,
            operation: self.operation,
        }
    }
    #[getter]
    fn style(&self) -> PyStyle {
        let s = &self.document.pages[self.page].operations[self.operation].style;
        PyStyle {
            pen: s.pen,
            color: s.color.clone(),
            width_mm: s.width_mm,
            line: s.line.clone().map(Into::into),
        }
    }
    #[getter]
    fn source(&self) -> PyProvenance {
        let s = &self.document.pages[self.page].operations[self.operation].source;
        PyProvenance {
            command_index: s.command_index,
            command: s.command.clone(),
            span: s.span.into(),
        }
    }
    #[getter]
    fn path_id(&self) -> usize {
        self.document.pages[self.page].operations[self.operation].path_id
    }
}

#[pyclass(name = "PlotPage", frozen, module = "ezhpgl._core")]
struct PyPage {
    document: Arc<core::PlotDocument>,
    index: usize,
}

#[pymethods]
impl PyPage {
    #[getter]
    fn omitted_content(&self) -> bool {
        self.document.pages[self.index].omitted_content
    }
    #[getter]
    fn plot_size_mm(&self) -> Option<(f64, f64)> {
        self.document.pages[self.index]
            .plot_size_mm
            .map(|s| (s[0], s[1]))
    }
    #[getter]
    fn index(&self) -> usize {
        self.index
    }
    #[getter]
    fn paper_size_mm(&self) -> Option<(f64, f64)> {
        self.document.pages[self.index]
            .paper_size_mm
            .map(|s| (s[0], s[1]))
    }
    #[getter]
    fn bounds(&self) -> Option<PyBounds> {
        self.document.pages[self.index].bounds.map(Into::into)
    }
    #[getter]
    fn operations(&self) -> Vec<PyOperation> {
        (0..self.document.pages[self.index].operations.len())
            .map(|operation| PyOperation {
                document: self.document.clone(),
                page: self.index,
                operation,
            })
            .collect()
    }
}

#[pyclass(name = "PlotDocument", frozen, module = "ezhpgl._core")]
struct PyPlotDocument {
    inner: Arc<core::PlotDocument>,
}

#[pymethods]
impl PyPlotDocument {
    #[getter]
    fn input_events(&self) -> Vec<PyInputEvent> {
        self.inner
            .input_events
            .iter()
            .cloned()
            .map(Into::into)
            .collect()
    }
    #[getter]
    fn device_profile(&self) -> PyDeviceProfile {
        PyDeviceProfile {
            inner: self.inner.device_profile,
        }
    }
    #[getter]
    fn schema_version(&self) -> u32 {
        self.inner.schema_version
    }
    #[getter]
    fn profile(&self) -> &'static str {
        self.inner.profile
    }
    #[getter]
    fn units(&self) -> &'static str {
        self.inner.units
    }
    #[getter]
    fn completeness(&self) -> &'static str {
        self.inner.completeness
    }
    #[getter]
    fn source_size(&self) -> usize {
        self.inner.source_size
    }
    #[getter]
    fn interpreted_commands(&self) -> usize {
        self.inner.interpreted_commands
    }
    #[getter]
    fn diagnostics(&self) -> Vec<PyDiagnostic> {
        self.inner.diagnostics.iter().map(Into::into).collect()
    }
    #[getter]
    fn raw(&self) -> Option<PyDocument> {
        self.inner
            .raw
            .as_ref()
            .map(|raw| PyDocument { inner: raw.clone() })
    }
    #[getter]
    fn final_state(&self) -> PyPlotState {
        let s = &self.inner.final_state;
        PyPlotState {
            position: s.position.into(),
            coordinate_mode: mode_name(s.coordinate_mode).into(),
            pen: s.pen,
            pen_down: s.pen_down,
            transform: s.transform.clone().map(Into::into),
            polygon_mode: s.polygon_mode,
            text: s.text.clone().map(Into::into),
            position_approximate: s.position_approximate,
        }
    }
    #[getter]
    fn pages(&self) -> Vec<PyPage> {
        (0..self.inner.pages.len())
            .map(|index| PyPage {
                document: self.inner.clone(),
                index,
            })
            .collect()
    }
    fn stats(&self) -> BTreeMap<&'static str, usize> {
        let s = self.inner.stats();
        BTreeMap::from([
            ("pages", s.pages),
            ("operations", s.operations),
            ("vertices", s.vertices),
            ("interpreted_commands", s.interpreted_commands),
            ("source_bytes", s.source_bytes),
        ])
    }
    fn to_json(&self, py: Python<'_>) -> PyResult<String> {
        py.detach(|| self.inner.to_json())
            .map_err(|e| PyValueError::new_err(e.to_string()))
    }
    #[pyo3(signature = (*, page=0, margin_mm=1.0))]
    fn to_svg(&self, py: Python<'_>, page: usize, margin_mm: f64) -> PyResult<String> {
        py.detach(|| self.inner.to_svg(page, margin_mm))
            .map_err(|e| PyValueError::new_err(e.to_string()))
    }
    fn write_json(&self, py: Python<'_>, path: PathBuf) -> PyResult<()> {
        let content = self.to_json(py)?;
        py.detach(|| std::fs::write(path, content))?;
        Ok(())
    }
    #[pyo3(signature = (path, *, page=0, margin_mm=1.0))]
    fn write_svg(
        &self,
        py: Python<'_>,
        path: PathBuf,
        page: usize,
        margin_mm: f64,
    ) -> PyResult<()> {
        let content = self.to_svg(py, page, margin_mm)?;
        py.detach(|| std::fs::write(path, content))?;
        Ok(())
    }
}

#[pyfunction]
#[pyo3(signature = (data, *, strict=true, preserve_commands=false, limits=None, geometry_limits=None, profile=None))]
#[allow(clippy::too_many_arguments)]
fn parse(
    py: Python<'_>,
    data: &[u8],
    strict: bool,
    preserve_commands: bool,
    limits: Option<&PyLimits>,
    geometry_limits: Option<&PyGeometryLimits>,
    profile: Option<&PyDeviceProfile>,
) -> PyResult<PyPlotDocument> {
    let options = core::InterpretOptions {
        parse: core::ParseOptions {
            strict,
            limits: limits.map_or_else(core::Limits::default, |l| l.inner),
        },
        preserve_commands,
        geometry_limits: geometry_limits.map_or_else(core::GeometryLimits::default, |l| l.inner),
        profile: profile.map_or_else(core::DeviceProfile::default, |p| p.inner),
    };
    let result = if data.len() > options.parse.limits.max_input_bytes {
        // Core checks before copying; the oversized slice never leaves this thread.
        core::parse(data, options)
    } else {
        let data: Arc<[u8]> = Arc::from(data);
        py.detach(move || core::parse_owned(data, options))
    };
    result
        .map(|inner| PyPlotDocument {
            inner: Arc::new(inner),
        })
        .map_err(|e| parse_error(py, e))
}

pub(super) fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    labels::register(m)?;
    m.add_class::<PyLineStyle>()?;
    m.add_class::<PyCurve>()?;
    m.add_class::<PyContour>()?;
    m.add_class::<PyContourEdge>()?;
    m.add_class::<PyFillStyle>()?;
    m.add_class::<PyDeviceProfile>()?;
    m.add_class::<PyClipRegion>()?;
    m.add_class::<PyTransformState>()?;
    m.add_class::<PyPoint>()?;
    m.add_class::<PyBounds>()?;
    m.add_class::<PyStyle>()?;
    m.add_class::<PyProvenance>()?;
    m.add_class::<PyPlotState>()?;
    m.add_class::<PyGeometryLimits>()?;
    m.add_class::<PyGeometry>()?;
    m.add_class::<PyOperation>()?;
    m.add_class::<PyPage>()?;
    m.add_class::<PyPlotDocument>()?;
    m.add_function(wrap_pyfunction!(parse, m)?)?;
    Ok(())
}
