use std::sync::Arc;

use serde::Serialize;

use crate::{ClipRegion, DeviceProfile, Diagnostic, RawDocument, SourceSpan, TransformState};

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct Bounds {
    pub min: Point,
    pub max: Point,
}

impl Bounds {
    pub(crate) fn include(&mut self, point: Point) {
        self.min.x = self.min.x.min(point.x);
        self.min.y = self.min.y.min(point.y);
        self.max.x = self.max.x.max(point.x);
        self.max.y = self.max.y.max(point.y);
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CoordinateMode {
    #[default]
    Absolute,
    Relative,
}

/// Snapshot in mm. The interpreter accumulates relative coordinates in plotter units.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct PlotState {
    pub position: Point,
    pub coordinate_mode: CoordinateMode,
    pub pen: u32,
    pub pen_down: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transform: Option<TransformState>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub polygon_mode: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<crate::TextState>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub position_approximate: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Style {
    /// Original logical pen ID; no device palette modulo is assumed.
    pub pen: u32,
    /// Monochrome preview defaults of the generic-plotter profile.
    pub color: String,
    pub width_mm: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<LineStyle>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct LineStyle {
    pub line_type: Option<i8>,
    pub pattern_length_mm: Option<f64>,
    pub phase_mm: f64,
    pub cap: u8,
    pub join: u8,
    pub miter_limit: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ChordTolerance {
    pub mode: u8,
    /// Degrees in mode 0, current user units in mode 1.
    pub value: f64,
}

/// Analytic physical-mm geometry, independent of the chord/preview points.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Curve {
    EllipticArc {
        center: Point,
        axis_u: Point,
        axis_v: Point,
        start_angle_deg: f64,
        sweep_angle_deg: f64,
        chord: ChordTolerance,
    },
    CubicBezier {
        control_points: [Point; 4],
        tolerance_mm: f64,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ContourEdge {
    /// Inclusive point indices in the containing contour.
    pub start: usize,
    pub end: usize,
    pub pen_down: bool,
    pub source: Provenance,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub curve: Option<Curve>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Contour {
    pub points: Vec<Point>,
    pub edges: Vec<ContourEdge>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct FillStyle {
    pub fill_type: u8,
    pub rule: &'static str,
    pub spacing_mm: Option<f64>,
    pub angle_deg: f64,
    /// Bounded hatch segments after polygon intersections, before IW clipping.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub hatch_lines: Vec<[Point; 2]>,
    /// Fixed-pattern phase at each segment start; empty for solid/adaptive lines.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub hatch_phases_mm: Vec<f64>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Geometry {
    /// One point with start_dot=true represents a parameterless PD dot.
    Polyline {
        points: Vec<Point>,
        start_dot: bool,
    },
    Curve {
        curve: Curve,
        points: Vec<Point>,
        start_dot: bool,
    },
    Polygon {
        contours: Arc<Vec<Contour>>,
        fill: Option<FillStyle>,
    },
    Label {
        label: crate::Label,
    },
}

impl Geometry {
    pub fn points(&self) -> &[Point] {
        match self {
            Self::Polyline { points, .. } | Self::Curve { points, .. } => points,
            Self::Polygon { .. } | Self::Label { .. } => &[],
        }
    }
    pub fn vertex_count(&self) -> usize {
        match self {
            Self::Label { label } => label.glyphs.len() * 4,
            Self::Polygon { contours, fill } => {
                contours.iter().map(|c| c.points.len()).sum::<usize>()
                    + fill.as_ref().map_or(0, |f| f.hatch_lines.len() * 2)
            }
            _ => self.points().len(),
        }
    }
    pub fn start_dot(&self) -> bool {
        matches!(
            self,
            Self::Polyline {
                start_dot: true,
                ..
            } | Self::Curve {
                start_dot: true,
                ..
            }
        )
    }
    pub fn curve(&self) -> Option<&Curve> {
        match self {
            Self::Curve { curve, .. } => Some(curve),
            _ => None,
        }
    }
    pub fn all_points(&self) -> Box<dyn Iterator<Item = &Point> + '_> {
        match self {
            Self::Label { label } => Box::new(
                label
                    .glyphs
                    .iter()
                    .filter(|g| !g.character.is_whitespace())
                    .flat_map(|g| g.envelope.iter()),
            ),
            Self::Polygon { contours, .. } => {
                Box::new(contours.iter().flat_map(|c| c.points.iter()))
            }
            _ => Box::new(self.points().iter()),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Provenance {
    pub command_index: usize,
    pub command: String,
    pub span: SourceSpan,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct PlotOperation {
    pub geometry: Geometry,
    pub style: Style,
    pub source: Provenance,
    /// Identifies continuous vectors, including those split across PA/PR/PD commands.
    pub path_id: usize,
    /// Geometry remains uncut. Apply this physical-mm clip to strokes and dots.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub clip: Option<ClipRegion>,
    /// Label fallback or geometry depending on approximate text cursor metrics.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub approximate: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct PlotPage {
    pub index: usize,
    /// Unspecified until a supported command/profile supplies paper dimensions.
    pub paper_size_mm: Option<[f64; 2]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plot_size_mm: Option<[f64; 2]>,
    /// Visible centerline/dot-center bounds; excludes pen-up travel and stroke thickness.
    pub bounds: Option<Bounds>,
    pub operations: Vec<PlotOperation>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub omitted_content: bool,
}

impl PlotPage {
    pub(crate) fn push(&mut self, operation: PlotOperation) {
        if let Some(b) = operation.visible_bounds() {
            crate::clip::include(&mut self.bounds, b.min);
            crate::clip::include(&mut self.bounds, b.max);
        }
        self.operations.push(operation);
    }

    pub(crate) fn paint_bounds(&self) -> Option<Bounds> {
        let mut result = None;
        for op in &self.operations {
            if let Some(b) = op.paint_bounds() {
                crate::clip::include(&mut result, b.min);
                crate::clip::include(&mut result, b.max);
            }
        }
        result
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct PlotStats {
    pub pages: usize,
    pub operations: usize,
    /// Includes repeated starting points and parameterless PD dots.
    pub vertices: usize,
    pub interpreted_commands: usize,
    pub source_bytes: usize,
}

#[derive(Clone, Debug, Serialize)]
pub struct PlotDocument {
    pub schema_version: u32,
    pub profile: &'static str,
    #[serde(skip_serializing_if = "DeviceProfile::is_default")]
    pub device_profile: DeviceProfile,
    pub units: &'static str,
    pub completeness: &'static str,
    pub source_size: usize,
    pub interpreted_commands: usize,
    pub pages: Vec<PlotPage>,
    pub diagnostics: Vec<Diagnostic>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub input_events: Vec<crate::InputEvent>,
    /// State immediately before the first failed/unsupported command, or at EOF.
    pub final_state: PlotState,
    /// Optional full raw parse, including lexical recovery beyond interpretation's stop.
    #[serde(skip)]
    pub raw: Option<Arc<RawDocument>>,
}

impl PlotDocument {
    pub fn stats(&self) -> PlotStats {
        PlotStats {
            pages: self.pages.len(),
            operations: self.pages.iter().map(|p| p.operations.len()).sum(),
            vertices: self
                .pages
                .iter()
                .flat_map(|p| &p.operations)
                .map(|op| op.geometry.vertex_count())
                .sum(),
            interpreted_commands: self.interpreted_commands,
            source_bytes: self.source_size,
        }
    }
}
