use crate::{Diagnostic, ParseError, Point, SourceSpan};

#[derive(Clone, Copy, Debug)]
pub(crate) enum PeEvent {
    Pen(u32),
    Move {
        point: Point,
        absolute: bool,
        up: bool,
    },
}

/// Byte decoder independent of plotter geometry. Values and flag arguments use
/// HP's signed-magnitude low-bit encoding (not zigzag/two's complement).
pub(crate) struct Decoder<'a> {
    data: &'a [u8],
    offset: usize,
    pos: usize,
    bits: u32,
    fraction: i32,
    absolute: bool,
    up: bool,
    values: usize,
    limit: usize,
}

impl<'a> Decoder<'a> {
    pub fn new(data: &'a [u8], offset: usize, limit: usize) -> Self {
        Self {
            data,
            offset,
            pos: 0,
            bits: 6,
            fraction: 0,
            absolute: false,
            up: false,
            values: 0,
            limit,
        }
    }
    fn fail(&self, code: &'static str, message: &str, start: usize) -> ParseError {
        ParseError {
            diagnostic: Diagnostic {
                code,
                message: message.into(),
                span: SourceSpan {
                    start: self.offset + start,
                    end: self.offset + self.pos,
                },
                command: Some("PE".into()),
                action: "stop_interpretation",
            },
        }
    }
    fn peek(&mut self) -> Option<u8> {
        while let Some(&raw) = self.data.get(self.pos) {
            let b = if self.bits == 5 { raw & 0x7f } else { raw };
            if b <= 32 || b == 127 || (128..=160).contains(&b) || b == 255 {
                self.pos += 1;
            } else {
                return Some(b);
            }
        }
        None
    }
    fn number(&mut self) -> Result<i64, ParseError> {
        let start = self.pos;
        if self.values >= self.limit {
            return Err(self.fail("LIMIT_EXCEEDED", "Decoded PE value limit exceeded", start));
        }
        self.values += 1;
        let mut word = 0u64;
        let mut shift = 0u32;
        loop {
            let Some(b) = self.peek() else {
                return Err(self.fail(
                    "PE_TRUNCATED",
                    "PE number is missing its final digit",
                    start,
                ));
            };
            self.pos += 1;
            let (digit, last) = match (self.bits, b) {
                (6, 63..=126) => (b - 63, false),
                (6, 191..=254) => (b - 191, true),
                (5, 63..=94) => (b - 63, false),
                (5, 95..=126) => (b - 95, true),
                _ => {
                    return Err(self.fail(
                        "PE_INVALID_BYTE",
                        "Invalid byte or flag inside a PE number",
                        start,
                    ))
                }
            };
            if shift >= 64 || u64::from(digit) > (u64::MAX >> shift) {
                return Err(self.fail("PE_OVERFLOW", "PE integer exceeds 64 encoded bits", start));
            }
            word |= u64::from(digit) << shift;
            if last {
                let magnitude = (word >> 1) as i64;
                return Ok(if word & 1 == 0 { magnitude } else { -magnitude });
            }
            shift += self.bits;
        }
    }
    pub fn next(&mut self) -> Result<Option<PeEvent>, ParseError> {
        loop {
            let start = self.pos;
            let Some(b) = self.peek() else {
                if self.absolute || self.up {
                    return Err(self.fail(
                        "PE_TRUNCATED",
                        "PE move flag has no coordinate pair",
                        start,
                    ));
                }
                return Ok(None);
            };
            match b & 0x7f {
                b':' => {
                    self.pos += 1;
                    let pen = self.number()?;
                    if !(0..=8_388_607).contains(&pen) {
                        return Err(self.fail(
                            "INVALID_PEN",
                            "PE pen ID is outside the profile range",
                            start,
                        ));
                    }
                    return Ok(Some(PeEvent::Pen(pen as u32)));
                }
                b'>' => {
                    self.pos += 1;
                    let fraction = self.number()?;
                    if !(-26..=26).contains(&fraction) {
                        return Err(self.fail(
                            "PE_FRACTION_RANGE",
                            "PE fractional bits must be between -26 and 26",
                            start,
                        ));
                    }
                    self.fraction = fraction as i32;
                }
                b'<' => {
                    self.pos += 1;
                    self.up = true;
                }
                b'=' => {
                    self.pos += 1;
                    self.absolute = true;
                }
                b'7' => {
                    self.pos += 1;
                    self.bits = 5;
                }
                _ => {
                    let divisor = 2f64.powi(self.fraction);
                    let x = self.number()? as f64 / divisor;
                    let y = self.number()? as f64 / divisor;
                    let event = PeEvent::Move {
                        point: Point { x, y },
                        absolute: self.absolute,
                        up: self.up,
                    };
                    self.absolute = false;
                    self.up = false;
                    return Ok(Some(event));
                }
            }
        }
    }
}
