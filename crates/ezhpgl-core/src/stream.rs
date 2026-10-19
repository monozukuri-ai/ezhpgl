use crate::{Diagnostic, ParseError, SourceSpan};

/// Original-input record outside HP-GL command payloads. Binary bytes are never searched.
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct InputEvent {
    pub kind: &'static str,
    pub span: SourceSpan,
    pub command: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payload_span: Option<SourceSpan>,
}

#[derive(Clone, Copy, PartialEq)]
pub(crate) enum Mode {
    Hpgl,
    Pcl,
    Pjl,
}

fn fail(code: &'static str, message: &str, start: usize, end: usize) -> ParseError {
    ParseError {
        diagnostic: Diagnostic {
            code,
            message: message.into(),
            span: SourceSpan { start, end },
            command: None,
            action: "stop",
        },
    }
}

pub(crate) fn next(
    data: &[u8],
    pos: &mut usize,
    mode: &mut Mode,
    limit: usize,
) -> Result<InputEvent, ParseError> {
    let start = *pos;
    let end_limit = start.saturating_add(limit).min(data.len());
    let remaining = &data[start..end_limit];
    let event = |kind, end, command: &str, payload_span| InputEvent {
        kind,
        span: SourceSpan { start, end },
        command: command.into(),
        payload_span,
    };
    if data[start..].starts_with(b"\x1b%-12345X") {
        if limit < 9 {
            return Err(fail(
                "LIMIT_EXCEEDED",
                "Container record byte limit exceeded",
                start,
                start + limit,
            ));
        }
        *pos += 9;
        *mode = Mode::Pjl;
        return Ok(event("uel", *pos, "UEL", None));
    }
    if *mode == Mode::Pjl {
        let Some(n) = remaining.iter().position(|b| *b == b'\n') else {
            return Err(fail(
                if end_limit < data.len() {
                    "LIMIT_EXCEEDED"
                } else {
                    "UNTERMINATED_PJL"
                },
                "PJL line requires a newline within the record limit",
                start,
                end_limit,
            ));
        };
        let line = &remaining[..n];
        if !line.is_ascii()
            || line
                .iter()
                .any(|b| b.is_ascii_control() && !matches!(b, b'\r' | b'\t'))
        {
            return Err(fail(
                "UNSUPPORTED_PJL",
                "Non-ASCII or control byte in PJL line",
                start,
                start + n,
            ));
        }
        let upper = String::from_utf8_lossy(line).to_ascii_uppercase();
        let words: Vec<_> = upper.split_ascii_whitespace().collect();
        if words.first() != Some(&"@PJL") {
            return Err(fail(
                "UNSUPPORTED_PJL",
                "Expected a PJL command line",
                start,
                start + n,
            ));
        }
        let command = words.get(1).copied().unwrap_or("");
        if matches!(command, "JOB" | "EOJ") && words.len() > 2 {
            let tail = words[2..].join(" ");
            let name = tail
                .strip_prefix("NAME")
                .and_then(|s| s.trim_start().strip_prefix('='))
                .map(str::trim)
                .and_then(|s| s.strip_prefix('"'))
                .and_then(|s| s.strip_suffix('"'));
            if name.is_none_or(|s| s.contains('"')) {
                return Err(fail("UNSUPPORTED_PJL", "JOB/EOJ only support an optional quoted NAME; page-selection settings are not ignored", start, start+n));
            }
        }
        let kind = match command {
            "" | "COMMENT" | "JOB" => "pjl_metadata",
            "EOJ" => "end_job",
            "ENTER" => {
                let tail = words[2..].join("");
                match tail.as_str() {
                    "LANGUAGE=HPGL2" => {
                        *mode = Mode::Hpgl;
                        "enter_hpgl_direct"
                    }
                    "LANGUAGE=PCL" => {
                        *mode = Mode::Pcl;
                        "enter_pcl"
                    }
                    _ => {
                        return Err(fail(
                            "UNSUPPORTED_PJL",
                            "Only ENTER LANGUAGE=HPGL2 or PCL is supported",
                            start,
                            start + n,
                        ))
                    }
                }
            }
            _ => {
                return Err(fail(
                    "UNSUPPORTED_PJL",
                    "PJL settings or language are outside this profile",
                    start,
                    start + n,
                ))
            }
        };
        *pos += n + 1;
        return Ok(event(kind, *pos, command, None));
    }
    if remaining.first() == Some(&12) && *mode == Mode::Pcl {
        *pos += 1;
        return Ok(event("page_break", *pos, "FF", None));
    }
    if remaining.first() != Some(&27) {
        if *mode != Mode::Pcl || remaining.is_empty() {
            return Err(fail(
                "LIMIT_EXCEEDED",
                "Container record byte limit exceeded",
                start,
                end_limit,
            ));
        }
        let count = remaining
            .iter()
            .position(|b| matches!(b, 12 | 27))
            .unwrap_or(remaining.len());
        if count == remaining.len() && end_limit < data.len() {
            return Err(fail(
                "LIMIT_EXCEEDED",
                "PCL text record exceeds limit",
                start,
                end_limit,
            ));
        }
        *pos += count;
        return Ok(event(
            if data[start..*pos].iter().all(u8::is_ascii_whitespace) {
                "pcl_whitespace"
            } else {
                "omitted_pcl_text"
            },
            *pos,
            "PCL text",
            Some(SourceSpan { start, end: *pos }),
        ));
    }
    if remaining.starts_with(b"\x1bE") {
        *pos += 2;
        *mode = Mode::Pcl;
        return Ok(event("reset", *pos, "ESC E", None));
    }
    if remaining.starts_with(b"\x1b%") {
        let Some(n) = remaining[2..]
            .iter()
            .position(u8::is_ascii_alphabetic)
            .map(|i| i + 2)
        else {
            return Err(fail(
                if end_limit < data.len() {
                    "LIMIT_EXCEEDED"
                } else {
                    "TRUNCATED_ESCAPE"
                },
                "Incomplete mode switch",
                start,
                end_limit,
            ));
        };
        let value = &remaining[2..n];
        if !(value.is_empty() || value == b"0") || !matches!(remaining[n], b'A' | b'B') {
            return Err(fail(
                "UNSUPPORTED_MODE_SWITCH",
                "Only mode 0 switches without cursor transfer are supported",
                start,
                start + n + 1,
            ));
        }
        *pos += n + 1;
        let previous = *mode;
        *mode = if remaining[n] == b'B' {
            Mode::Hpgl
        } else {
            Mode::Pcl
        };
        return Ok(event(
            if previous == *mode {
                "mode_noop"
            } else if *mode == Mode::Hpgl {
                "enter_hpgl"
            } else {
                "enter_pcl"
            },
            *pos,
            if *mode == Mode::Hpgl {
                "ESC %0B"
            } else {
                "ESC %0A"
            },
            None,
        ));
    }
    if *mode != Mode::Pcl {
        return Err(fail(
            "UNSUPPORTED_CONTAINER_COMMAND",
            "Unknown escape at an HP-GL command boundary",
            start,
            (start + 2).min(data.len()),
        ));
    }
    if remaining.len() < 4 {
        return Err(fail(
            if end_limit < data.len() {
                "LIMIT_EXCEEDED"
            } else {
                "TRUNCATED_ESCAPE"
            },
            "Incomplete PCL escape",
            start,
            end_limit,
        ));
    }
    // A narrow allow-list of non-combined raster commands. No suffix guessing.
    let group = &remaining[1..3];
    let mut n = 3;
    while n < remaining.len() && remaining[n].is_ascii_digit() {
        n += 1;
    }
    if n == remaining.len() {
        return Err(fail(
            if end_limit < data.len() {
                "LIMIT_EXCEEDED"
            } else {
                "TRUNCATED_ESCAPE"
            },
            "Incomplete PCL parameter",
            start,
            end_limit,
        ));
    }
    let term = remaining[n];
    let number = if n == 3 {
        0
    } else {
        std::str::from_utf8(&remaining[3..n])
            .unwrap()
            .parse::<usize>()
            .map_err(|_| fail("LIMIT_EXCEEDED", "PCL parameter overflow", start, start + n))?
    };
    let command = format!("{}{}", String::from_utf8_lossy(group), char::from(term));
    if group == b"*b" && matches!(term, b'W' | b'V') {
        let payload_start = start + n + 1;
        let end = payload_start.checked_add(number).ok_or_else(|| {
            fail(
                "LIMIT_EXCEEDED",
                "Binary length overflow",
                start,
                payload_start,
            )
        })?;
        if end - start > limit {
            return Err(fail(
                "LIMIT_EXCEEDED",
                "Binary record exceeds byte limit",
                start,
                payload_start,
            ));
        }
        if end > data.len() {
            return Err(fail(
                "TRUNCATED_BINARY",
                "PCL binary payload is shorter than declared",
                payload_start,
                data.len(),
            ));
        }
        *pos = end;
        return Ok(event(
            "omitted_raster_data",
            end,
            &command,
            Some(SourceSpan {
                start: payload_start,
                end,
            }),
        ));
    }
    if (group == b"*r" && matches!(term, b'A' | b'B' | b'C' | b'S' | b'T'))
        || (group == b"*b" && term == b'M')
        || (group == b"*t" && term == b'R')
    {
        *pos += n + 1;
        return Ok(event("omitted_raster_control", *pos, &command, None));
    }
    Err(fail(
        "UNSUPPORTED_PCL",
        "Unsupported PCL command; its payload/state is not guessed",
        start,
        start + n + 1,
    ))
}
