use std::fmt;

use crate::SourceSpan;

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct Diagnostic {
    pub code: &'static str,
    pub message: String,
    pub span: SourceSpan,
    pub command: Option<String>,
    pub action: &'static str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParseError {
    pub diagnostic: Diagnostic,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} at bytes {}..{}: {}",
            self.diagnostic.code,
            self.diagnostic.span.start,
            self.diagnostic.span.end,
            self.diagnostic.message
        )
    }
}

impl std::error::Error for ParseError {}
