use std::sync::Arc;

use crate::Diagnostic;

/// Half-open byte range in the original input, including any caller-provided prefix.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize)]
pub struct SourceSpan {
    pub start: usize,
    pub end: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Number {
    pub value: f64,
    pub span: SourceSpan,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Payload {
    Numbers(Vec<Number>),
    Label {
        terminator: u8,
        print_terminator: bool,
    },
    EncodedPolyline,
    Comment,
    LabelTerminator {
        terminator: u8,
        print_terminator: bool,
    },
    /// Syntax not implemented; only a safely delimited raw payload is retained.
    Opaque,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Command {
    pub name: [u8; 2],
    pub payload: Payload,
    pub span: SourceSpan,
    /// Excludes the mnemonic and payload delimiters (quotes, label terminator, semicolon).
    pub payload_span: SourceSpan,
}

impl Command {
    pub fn name(&self) -> &str {
        // Mnemonics are constructed exclusively from ASCII letters.
        std::str::from_utf8(&self.name).expect("ASCII mnemonic invariant")
    }

    pub fn kind(&self) -> &'static str {
        match self.payload {
            Payload::Numbers(_) => "numbers",
            Payload::Label { .. } => "label",
            Payload::EncodedPolyline => "encoded_polyline",
            Payload::Comment => "comment",
            Payload::LabelTerminator { .. } => "label_terminator",
            Payload::Opaque => "opaque",
        }
    }
}

#[derive(Clone, Debug)]
pub struct RawDocument {
    pub source: Arc<[u8]>,
    pub commands: Vec<Command>,
    pub diagnostics: Vec<Diagnostic>,
    /// Lexical completeness only; does not imply semantic or rendering support.
    pub complete: bool,
    /// Only populated by the high-level container-aware entrypoint.
    pub input_events: Vec<crate::InputEvent>,
}

#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub max_input_bytes: usize,
    pub max_command_bytes: usize,
    pub max_commands: usize,
    pub max_arguments_per_command: usize,
    pub max_diagnostics: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_input_bytes: 64 * 1024 * 1024,
            max_command_bytes: 4 * 1024 * 1024,
            max_commands: 1_000_000,
            max_arguments_per_command: 262_144,
            max_diagnostics: 1024,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct ParseOptions {
    pub strict: bool,
    pub limits: Limits,
}

impl Default for ParseOptions {
    fn default() -> Self {
        Self {
            strict: true,
            limits: Limits::default(),
        }
    }
}

pub(crate) fn is_numeric_command(name: &[u8; 2]) -> bool {
    matches!(
        name,
        b"IN"
            | b"DF"
            | b"SP"
            | b"PA"
            | b"PR"
            | b"PU"
            | b"PD"
            | b"PG"
            | b"IP"
            | b"IR"
            | b"SC"
            | b"RO"
            | b"IW"
            | b"CI"
            | b"AA"
            | b"AR"
            | b"AT"
            | b"RT"
            | b"BZ"
            | b"BR"
            | b"PW"
            | b"WU"
            | b"PC"
            | b"NP"
            | b"LT"
            | b"LA"
            | b"PM"
            | b"FP"
            | b"EP"
            | b"FT"
            | b"RA"
            | b"RR"
            | b"EA"
            | b"ER"
            | b"WG"
            | b"EW"
            | b"SI"
            | b"SR"
            | b"DI"
            | b"DR"
            | b"LO"
            | b"SL"
            | b"AD"
            | b"SD"
            | b"SS"
            | b"SA"
            | b"CP"
            | b"ES"
            | b"DV"
            | b"CF"
            | b"SB"
            | b"FI"
            | b"FN"
            | b"TD"
            | b"AC"
            | b"UL"
            | b"RF"
            | b"TR"
            | b"SV"
            | b"MC"
            | b"CR"
            | b"PS"
            | b"RP"
            | b"VS"
            | b"FS"
            | b"PT"
            | b"TL"
            | b"XT"
            | b"YT"
            | b"CT"
            | b"CS"
            | b"CA"
            | b"IM"
            | b"DC"
            | b"DP"
            | b"EC"
            | b"AF"
            | b"AH"
            | b"NR"
            | b"OE"
            | b"OA"
            | b"OC"
            | b"OD"
            | b"OF"
            | b"OI"
            | b"OH"
            | b"OO"
            | b"OP"
            | b"OS"
            | b"OW"
    )
}

pub(crate) fn is_known_command(name: &[u8; 2]) -> bool {
    is_numeric_command(name) || matches!(name, b"LB" | b"DT" | b"CO" | b"PE" | b"BP")
}
