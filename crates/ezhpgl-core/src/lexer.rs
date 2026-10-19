use std::sync::Arc;

use crate::{
    command::is_numeric_command, detect_format, Command, Diagnostic, Number, ParseError,
    ParseOptions, Payload, RawDocument, SourceSpan,
};

/// Parse a standalone HP-GL-family command stream, retaining the supplied buffer.
/// Use the high-level parser for container framing and command interpretation.
pub fn parse_commands(data: &[u8], options: ParseOptions) -> Result<RawDocument, ParseError> {
    if data.len() > options.limits.max_input_bytes {
        return Err(ParseError {
            diagnostic: Diagnostic {
                code: "LIMIT_EXCEEDED",
                message: "Input byte limit exceeded".into(),
                span: SourceSpan {
                    start: 0,
                    end: data.len(),
                },
                command: None,
                action: "stop",
            },
        });
    }
    parse_owned_commands(Arc::from(data), options)
}

/// Parse a shared owned buffer without copying it, for bindings and retained inputs.
pub fn parse_owned_commands(
    data: Arc<[u8]>,
    options: ParseOptions,
) -> Result<RawDocument, ParseError> {
    parse_input(data, options, false)
}

pub(crate) fn parse_input(
    data: Arc<[u8]>,
    options: ParseOptions,
    containers: bool,
) -> Result<RawDocument, ParseError> {
    let mut parser = Parser {
        data,
        options,
        pos: 0,
        start: 0,
        name: None,
        terminator: 3,
        print_terminator: false,
        diagnostics: Vec::new(),
    };
    if parser.data.len() > options.limits.max_input_bytes {
        return Err(parser.error(
            "LIMIT_EXCEEDED",
            "Input byte limit exceeded",
            0,
            parser.data.len(),
        ));
    }
    let detection = detect_format(&parser.data);
    if matches!(detection.container, "pdf" | "postscript")
        || (!containers && matches!(detection.container, "pjl" | "pcl_or_control"))
    {
        return Err(parser.error(
            "UNSUPPORTED_CONTAINER",
            "Unsupported container for this API",
            0,
            parser.data.len().min(16),
        ));
    }
    let mut commands = Vec::new();
    let mut events = Vec::new();
    let mut mode = match detection.container {
        "pjl" => crate::stream::Mode::Pjl,
        "pcl_or_control" => crate::stream::Mode::Pcl,
        _ => crate::stream::Mode::Hpgl,
    };
    loop {
        if mode == crate::stream::Mode::Hpgl {
            parser.skip_padding();
        } else if mode == crate::stream::Mode::Pjl {
            while parser.peek().is_some_and(|b| b.is_ascii_whitespace()) {
                parser.pos += 1;
            }
        }
        if parser.pos == parser.data.len() {
            break;
        }
        parser.start = parser.pos;
        parser.name = None;
        if commands.len() + events.len() >= options.limits.max_commands {
            return Err(parser.error(
                "LIMIT_EXCEEDED",
                "Command count limit exceeded",
                parser.pos,
                parser.pos,
            ));
        }
        if containers && (mode != crate::stream::Mode::Hpgl || parser.peek() == Some(27)) {
            match crate::stream::next(
                &parser.data,
                &mut parser.pos,
                &mut mode,
                options.limits.max_command_bytes,
            ) {
                Ok(event) => {
                    if matches!(event.kind, "reset" | "end_job") {
                        parser.terminator = 3;
                        parser.print_terminator = false;
                    }
                    events.push(event);
                }
                Err(e) => {
                    if options.strict || e.diagnostic.code == "LIMIT_EXCEEDED" {
                        return Err(e);
                    }
                    parser.record(e.diagnostic)?;
                    break;
                }
            }
            continue;
        }
        match parser.command() {
            Ok(command) => commands.push(command),
            Err(mut error) => {
                if options.strict || error.diagnostic.code == "LIMIT_EXCEEDED" {
                    return Err(error);
                }
                // Missing special terminators, invalid DT, or escape/binary bytes may
                // change framing. Never guess a new command boundary in those cases.
                let can_recover = matches!(
                    error.diagnostic.code,
                    "INVALID_NUMBER"
                        | "EXPONENT_NOT_ALLOWED"
                        | "MISSING_PARAMETER"
                        | "INVALID_RESET"
                );
                if can_recover && parser.recover_numeric()? {
                    error.diagnostic.action = "skip_command";
                    parser.record(error.diagnostic)?;
                } else {
                    error.diagnostic.action = "stop";
                    parser.record(error.diagnostic)?;
                    break;
                }
            }
        }
    }
    Ok(RawDocument {
        source: parser.data,
        commands,
        complete: parser.diagnostics.is_empty(),
        diagnostics: parser.diagnostics,
        input_events: events,
    })
}

struct Parser {
    data: Arc<[u8]>,
    options: ParseOptions,
    pos: usize,
    start: usize,
    name: Option<[u8; 2]>,
    terminator: u8,
    print_terminator: bool,
    diagnostics: Vec<Diagnostic>,
}

impl Parser {
    fn error(&self, code: &'static str, message: &str, start: usize, end: usize) -> ParseError {
        ParseError {
            diagnostic: Diagnostic {
                code,
                message: message.into(),
                span: SourceSpan { start, end },
                command: self.name.map(|n| String::from_utf8_lossy(&n).into_owned()),
                action: "stop",
            },
        }
    }

    fn peek(&self) -> Option<u8> {
        self.data.get(self.pos).copied()
    }

    fn bump(&mut self) -> Result<(), ParseError> {
        if self.pos - self.start >= self.options.limits.max_command_bytes {
            return Err(self.error(
                "LIMIT_EXCEEDED",
                "Command byte limit exceeded",
                self.start,
                self.pos,
            ));
        }
        self.pos += 1;
        Ok(())
    }

    fn skip_padding(&mut self) {
        while self
            .peek()
            .is_some_and(|b| b.is_ascii_whitespace() || b == b';')
        {
            self.pos += 1;
        }
    }

    fn record(&mut self, diagnostic: Diagnostic) -> Result<(), ParseError> {
        if self.diagnostics.len() >= self.options.limits.max_diagnostics {
            return Err(self.error(
                "LIMIT_EXCEEDED",
                "Diagnostic count limit exceeded",
                self.start,
                self.pos,
            ));
        }
        self.diagnostics.push(diagnostic);
        Ok(())
    }

    fn command(&mut self) -> Result<Command, ParseError> {
        let Some(pair) = self.data.get(self.pos..self.pos.saturating_add(2)) else {
            return Err(self.error(
                "INVALID_MNEMONIC",
                "Expected a two-letter mnemonic",
                self.pos,
                self.data.len(),
            ));
        };
        if !pair.iter().all(u8::is_ascii_alphabetic) {
            let code = if pair[0] == 27 {
                "UNSUPPORTED_CONTAINER"
            } else {
                "INVALID_MNEMONIC"
            };
            return Err(self.error(
                code,
                "Expected a two-letter mnemonic; no byte resynchronization attempted",
                self.pos,
                self.pos + 2,
            ));
        }
        let name = [pair[0].to_ascii_uppercase(), pair[1].to_ascii_uppercase()];
        self.name = Some(name);
        self.bump()?;
        self.bump()?;
        let payload_start = self.pos;
        let (payload, payload_span) = match &name {
            b"LB" => {
                let span = self.until(self.terminator, "UNTERMINATED_LABEL")?;
                (
                    Payload::Label {
                        terminator: self.terminator,
                        print_terminator: self.print_terminator,
                    },
                    span,
                )
            }
            b"PE" => (
                Payload::EncodedPolyline,
                self.until(b';', "UNTERMINATED_PE")?,
            ),
            b"CO" => {
                while self.peek().is_some_and(|b| b.is_ascii_whitespace()) {
                    self.bump()?;
                }
                if self.peek() != Some(b'"') {
                    return Err(self.error(
                        "INVALID_COMMENT",
                        "CO requires a double-quoted payload",
                        self.pos,
                        self.pos,
                    ));
                }
                self.bump()?;
                let span = self.until(b'"', "UNTERMINATED_COMMENT")?;
                self.finish_delimited()?;
                (Payload::Comment, span)
            }
            b"DT" => self.define_terminator()?,
            b"BP" if self.bp_without_parameters() => {
                let (numbers, end) = self.numbers()?;
                self.terminator = 3;
                self.print_terminator = false;
                (
                    Payload::Numbers(numbers),
                    SourceSpan {
                        start: payload_start,
                        end,
                    },
                )
            }
            _ if is_numeric_command(&name) => {
                let (numbers, end) = self.numbers()?;
                if matches!(&name, b"IN" | b"DF") {
                    if !numbers.is_empty() {
                        return Err(self.error(
                            "INVALID_RESET",
                            "IN and DF do not accept parameters",
                            payload_start,
                            end,
                        ));
                    }
                    self.terminator = 3;
                    self.print_terminator = false;
                }
                (
                    Payload::Numbers(numbers),
                    SourceSpan {
                        start: payload_start,
                        end,
                    },
                )
            }
            _ => {
                if self.options.strict {
                    return Err(self.error(
                        "UNSUPPORTED_SYNTAX",
                        "Unknown or unsupported command grammar",
                        self.start,
                        self.pos,
                    ));
                }
                let span = self.opaque()?;
                let mut diagnostic = self
                    .error(
                        "UNSUPPORTED_SYNTAX",
                        "Retained opaque command through a semicolon; semantics unknown",
                        self.start,
                        self.pos,
                    )
                    .diagnostic;
                diagnostic.action = "preserve_opaque";
                self.record(diagnostic)?;
                (Payload::Opaque, span)
            }
        };
        Ok(Command {
            name,
            payload,
            span: SourceSpan {
                start: self.start,
                end: self.pos,
            },
            payload_span,
        })
    }

    fn until(&mut self, terminator: u8, code: &'static str) -> Result<SourceSpan, ParseError> {
        let start = self.pos;
        while let Some(b) = self.peek() {
            if b == terminator {
                let end = self.pos;
                self.bump()?;
                return Ok(SourceSpan { start, end });
            }
            self.bump()?;
        }
        Err(self.error(code, "Payload terminator is missing", start, self.pos))
    }

    fn bp_without_parameters(&self) -> bool {
        // The corpus uses BPIN... with omitted terminators. Only BP's empty form
        // is supported; its kind/value/title grammar remains explicitly opaque.
        self.data[self.pos..]
            .iter()
            .take(self.options.limits.max_command_bytes)
            .find(|b| !b.is_ascii_whitespace())
            .is_none_or(|b| b.is_ascii_alphabetic() || *b == b';')
    }

    fn finish_delimited(&mut self) -> Result<(), ParseError> {
        if self.peek() == Some(b';') {
            self.bump()?;
        } else if self
            .peek()
            .is_some_and(|b| !b.is_ascii_whitespace() && !b.is_ascii_alphabetic())
        {
            return Err(self.error(
                "INVALID_DELIMITER",
                "Unexpected bytes after delimited payload",
                self.pos,
                self.pos + 1,
            ));
        }
        Ok(())
    }

    fn define_terminator(&mut self) -> Result<(Payload, SourceSpan), ParseError> {
        let start = self.pos;
        let Some(terminator) = self.peek() else {
            return Err(self.error(
                "INVALID_DT",
                "DT requires a terminator byte or semicolon",
                start,
                start,
            ));
        };
        if terminator == b';' {
            self.bump()?;
            self.terminator = 3;
            self.print_terminator = false;
            return Ok((
                Payload::LabelTerminator {
                    terminator: 3,
                    print_terminator: false,
                },
                SourceSpan { start, end: start },
            ));
        }
        if matches!(terminator, 0 | 10 | 27) {
            return Err(self.error("INVALID_DT", "Invalid label terminator", start, start + 1));
        }
        self.bump()?;
        let mut print_terminator = false;
        let before_padding = self.pos;
        while self.peek().is_some_and(|b| b.is_ascii_whitespace()) {
            self.bump()?;
        }
        let had_padding = self.pos > before_padding;
        let comma = self.peek() == Some(b',');
        let spaced_mode = had_padding
            && self
                .peek()
                .is_some_and(|b| b.is_ascii_digit() || matches!(b, b'+' | b'-' | b'.'));
        if comma || spaced_mode {
            if comma {
                self.bump()?;
            }
            let (values, _) = self.numbers().map_err(|mut error| {
                if error.diagnostic.code != "LIMIT_EXCEEDED" {
                    error.diagnostic.code = "INVALID_DT";
                }
                error
            })?;
            if values.len() != 1 || !matches!(values[0].value, 0.0 | 1.0) {
                return Err(self.error("INVALID_DT", "DT mode must be 0 or 1", start, self.pos));
            }
            print_terminator = values[0].value == 0.0;
        } else {
            // Whitespace immediately after DT is itself the terminator. Only
            // subsequent whitespace is padding. A following mnemonic may terminate DT.
            if self.peek().is_none() && !had_padding {
                return Err(self.error(
                    "MISSING_TERMINATOR",
                    "DT has no command terminator",
                    start,
                    self.pos,
                ));
            }
            self.finish_delimited()?;
        }
        let end = if self.pos > start && self.data[self.pos - 1] == b';' {
            self.pos - 1
        } else {
            self.pos
        };
        self.terminator = terminator;
        self.print_terminator = print_terminator;
        Ok((
            Payload::LabelTerminator {
                terminator,
                print_terminator,
            },
            SourceSpan { start, end },
        ))
    }

    fn numbers(&mut self) -> Result<(Vec<Number>, usize), ParseError> {
        let mut values = Vec::new();
        let mut need_value = false;
        let mut had_separator = true;
        let mut trailing_space = false;
        loop {
            match self.peek() {
                Some(b) if b.is_ascii_whitespace() => {
                    self.bump()?;
                    had_separator = true;
                    trailing_space = true;
                }
                Some(b';') => {
                    if need_value {
                        return Err(self.error(
                            "MISSING_PARAMETER",
                            "Empty numeric parameter",
                            self.pos,
                            self.pos + 1,
                        ));
                    }
                    let end = self.pos;
                    self.bump()?;
                    return Ok((values, end));
                }
                Some(b) if b.is_ascii_alphabetic() => {
                    if need_value {
                        return Err(self.error(
                            "MISSING_PARAMETER",
                            "Empty numeric parameter before mnemonic",
                            self.pos,
                            self.pos,
                        ));
                    }
                    return Ok((values, self.pos));
                }
                None => {
                    if need_value {
                        return Err(self.error(
                            "MISSING_PARAMETER",
                            "Empty trailing parameter",
                            self.pos,
                            self.pos,
                        ));
                    }
                    if trailing_space {
                        return Ok((values, self.pos));
                    }
                    return Err(self.error(
                        "MISSING_TERMINATOR",
                        "Final command requires a terminator",
                        self.pos,
                        self.pos,
                    ));
                }
                Some(b',') => {
                    if values.is_empty() || need_value {
                        return Err(self.error(
                            "MISSING_PARAMETER",
                            "Empty numeric parameter",
                            self.pos,
                            self.pos + 1,
                        ));
                    }
                    self.bump()?;
                    need_value = true;
                    had_separator = true;
                    trailing_space = false;
                }
                Some(b) if b.is_ascii_digit() || matches!(b, b'+' | b'-' | b'.') => {
                    if !had_separator && !matches!(b, b'+' | b'-') {
                        return Err(self.error(
                            "INVALID_NUMBER",
                            "Missing numeric separator",
                            self.pos,
                            self.pos + 1,
                        ));
                    }
                    if values.len() >= self.options.limits.max_arguments_per_command {
                        return Err(self.error(
                            "LIMIT_EXCEEDED",
                            "Argument count limit exceeded",
                            self.start,
                            self.pos,
                        ));
                    }
                    values.push(self.number()?);
                    need_value = false;
                    had_separator = false;
                    trailing_space = false;
                }
                _ => {
                    return Err(self.error(
                        "INVALID_NUMBER",
                        "Unexpected non-numeric byte",
                        self.pos,
                        self.pos + 1,
                    ))
                }
            }
        }
    }

    fn number(&mut self) -> Result<Number, ParseError> {
        let start = self.pos;
        if self.peek().is_some_and(|b| matches!(b, b'+' | b'-')) {
            self.bump()?;
        }
        let mut digits = 0;
        while self.peek().is_some_and(|b| b.is_ascii_digit()) {
            digits += 1;
            self.bump()?;
        }
        if self.peek() == Some(b'.') {
            self.bump()?;
            while self.peek().is_some_and(|b| b.is_ascii_digit()) {
                digits += 1;
                self.bump()?;
            }
        }
        if digits == 0 {
            return Err(self.error("INVALID_NUMBER", "Expected decimal digits", start, self.pos));
        }
        if self.peek().is_some_and(|b| matches!(b, b'e' | b'E'))
            && self
                .data
                .get(self.pos + 1)
                .is_some_and(|b| b.is_ascii_digit() || matches!(b, b'+' | b'-'))
        {
            return Err(self.error(
                "EXPONENT_NOT_ALLOWED",
                "HP-GL numbers use fixed-point decimal syntax",
                start,
                self.pos + 1,
            ));
        }
        let text =
            std::str::from_utf8(&self.data[start..self.pos]).expect("ASCII number invariant");
        let value = text
            .parse::<f64>()
            .map_err(|_| self.error("INVALID_NUMBER", "Invalid decimal number", start, self.pos))?;
        if !value.is_finite() {
            return Err(self.error(
                "INVALID_NUMBER",
                "Non-finite numeric value",
                start,
                self.pos,
            ));
        }
        Ok(Number {
            value,
            span: SourceSpan {
                start,
                end: self.pos,
            },
        })
    }

    fn opaque(&mut self) -> Result<SourceSpan, ParseError> {
        let start = self.pos;
        let mut quoted = false;
        while let Some(b) = self.peek() {
            if b == 27 {
                return Err(self.error(
                    "UNSUPPORTED_CONTAINER",
                    "Escape inside unknown command; framing is uncertain",
                    self.pos,
                    self.pos + 1,
                ));
            }
            if b == b'"' {
                quoted = !quoted;
            }
            if b == b';' && !quoted {
                let end = self.pos;
                self.bump()?;
                return Ok(SourceSpan { start, end });
            }
            self.bump()?;
        }
        Err(self.error(
            "UNTERMINATED_OPAQUE",
            "Unknown command has no safe semicolon boundary",
            start,
            self.pos,
        ))
    }

    fn recover_numeric(&mut self) -> Result<bool, ParseError> {
        // Scan only a numeric-looking tail. An alphabetic sequence might be LB,
        // CO or DT, so never skip across it looking for a semicolon inside data.
        // Exponent characters are allowed solely to discard a malformed number.
        if self.pos > self.start && self.data[self.pos - 1] == b';' {
            return Ok(true);
        }
        while let Some(b) = self.peek() {
            if b == b';' {
                self.bump()?;
                return Ok(true);
            }
            if !(b.is_ascii_whitespace()
                || b.is_ascii_digit()
                || matches!(b, b'+' | b'-' | b'.' | b',' | b'e' | b'E'))
            {
                return Ok(false);
            }
            self.bump()?;
        }
        Ok(false)
    }
}
