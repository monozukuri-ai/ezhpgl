//! HP-GL-family parsing and a bounded basic plot interpreter.
#![forbid(unsafe_code)]

mod attributes;
mod clip;
mod command;
mod curves;
mod detect;
mod diagnostics;
mod document;
mod export;
mod interpreter;
mod lexer;
mod pe;
mod polygon;
mod stream;
mod text;
mod transform;

pub use command::{Command, Limits, Number, ParseOptions, Payload, RawDocument, SourceSpan};
pub use detect::{detect_format, DetectionResult};
pub use diagnostics::{Diagnostic, ParseError};
pub use document::{
    Bounds, ChordTolerance, Contour, ContourEdge, CoordinateMode, Curve, FillStyle, Geometry,
    LineStyle, PlotDocument, PlotOperation, PlotPage, PlotState, PlotStats, Point, Provenance,
    Style,
};
pub use export::ExportError;
pub use interpreter::{parse, parse_owned, GeometryLimits, InterpretOptions};
pub use lexer::{parse_commands, parse_owned_commands};
pub use stream::InputEvent;
pub use text::{FontSpec, Glyph, Label, TextState};
pub use transform::{ClipRegion, DeviceProfile, TransformState};

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
