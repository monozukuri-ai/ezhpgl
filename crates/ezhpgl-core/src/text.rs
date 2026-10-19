use crate::curves::{add, length, mul, physical_ok, sub};
use crate::transform::{Space, TransformError};
use crate::{Point, SourceSpan};

const INVALID: TransformError = ("INVALID_ARGUMENTS", "Invalid text attribute arguments");
const UNSUPPORTED: TransformError = (
    "UNSUPPORTED_TEXT_ATTRIBUTE",
    "Text attribute is outside the bounded 8-bit preview profile",
);

#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize)]
pub struct FontSpec {
    pub symbol_set: u16,
    pub spacing: u8,
    pub pitch: f64,
    pub height_points: f64,
    pub posture: u16,
    pub weight: i16,
    pub typeface: u16,
}
impl Default for FontSpec {
    fn default() -> Self {
        Self {
            symbol_set: 277,
            spacing: 0,
            pitch: 9.,
            height_points: 11.5,
            posture: 0,
            weight: 0,
            typeface: 48,
        }
    }
}

#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct TextState {
    pub standard: FontSpec,
    pub alternate: FontSpec,
    pub use_alternate: bool,
    pub size_mode: &'static str,
    pub width: f64,
    pub height: f64,
    pub direction: Point,
    pub relative_direction: bool,
    pub origin: u8,
    pub slant: f64,
    pub extra_space: Point,
    pub path: u8,
    pub line: u8,
    pub transparent: bool,
    /// Physical millimeters, independent of SC.
    pub carriage_return: Point,
}
impl Default for TextState {
    fn default() -> Self {
        Self {
            standard: FontSpec::default(),
            alternate: FontSpec::default(),
            use_alternate: false,
            size_mode: "font",
            width: 0.,
            height: 0.,
            direction: Point { x: 1., y: 0. },
            relative_direction: false,
            origin: 1,
            slant: 0.,
            extra_space: Point::default(),
            path: 0,
            line: 0,
            transparent: false,
            carriage_return: Point::default(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct Glyph {
    pub character: char,
    pub source_offset: usize,
    pub origin: Point,
    pub axis_x: Point,
    pub axis_y: Point,
    pub font: FontSpec,
    /// Conservative fallback-font envelope, not native glyph outlines.
    pub envelope: [Point; 4],
}

#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct Label {
    pub data: Vec<u8>,
    pub payload_span: SourceSpan,
    pub terminator: u8,
    pub print_terminator: bool,
    pub decoded: String,
    pub glyphs: Vec<Glyph>,
    pub unmapped_offsets: Vec<usize>,
    pub state: TextState,
    pub start: Point,
    pub end: Point,
    pub approximate: bool,
}

impl TextState {
    pub(crate) fn command(
        &mut self,
        name: &str,
        a: &[f64],
        position: Point,
    ) -> Result<(), TransformError> {
        if a.iter().any(|v| !v.is_finite() || v.abs() > 1_073_741_824.) {
            return Err(INVALID);
        }
        let integer = |v: f64| (v + 0.5).floor();
        let clamped = |v: f64| v.clamp(-32768., 32767.);
        match name {
            "SI" | "SR" => {
                if !matches!(a.len(), 0 | 2) {
                    return Err(INVALID);
                }
                if name == "SI" && a.is_empty() {
                    self.size_mode = "font";
                    self.width = 0.;
                    self.height = 0.;
                } else {
                    self.size_mode = if name == "SI" { "absolute" } else { "relative" };
                    self.width = clamped(a.first().copied().unwrap_or(0.75));
                    self.height = clamped(a.get(1).copied().unwrap_or(1.5));
                }
            }
            "DI" | "DR" => {
                if !matches!(a.len(), 0 | 2) {
                    return Err(INVALID);
                }
                self.direction = Point {
                    x: clamped(a.first().copied().unwrap_or(1.)),
                    y: clamped(a.get(1).copied().unwrap_or(0.)),
                };
                if length(self.direction) == 0. {
                    return Err(INVALID);
                }
                self.relative_direction = name == "DR";
                self.carriage_return = position;
            }
            "LO" => {
                if a.len() > 1 {
                    return Err(INVALID);
                }
                let v = integer(a.first().copied().unwrap_or(1.));
                if !(1. ..=9.).contains(&v) && !(11. ..=19.).contains(&v) {
                    return Err(UNSUPPORTED);
                }
                self.origin = v as u8;
                self.carriage_return = position;
            }
            "SL" => {
                if a.len() > 1 {
                    return Err(INVALID);
                }
                self.slant = clamped(a.first().copied().unwrap_or(0.));
            }
            "ES" => {
                if a.len() > 2 {
                    return Err(INVALID);
                }
                self.extra_space = Point {
                    x: clamped(a.first().copied().unwrap_or(0.)),
                    y: clamped(a.get(1).copied().unwrap_or(0.)),
                };
                if self.extra_space.x <= -1. || self.extra_space.y <= -1. {
                    return Err(UNSUPPORTED);
                }
            }
            "DV" => {
                if a.len() > 2 {
                    return Err(INVALID);
                }
                let path = integer(a.first().copied().unwrap_or(0.));
                let line = integer(a.get(1).copied().unwrap_or(0.));
                if !(0. ..=3.).contains(&path) || !(0. ..=1.).contains(&line) {
                    return Err(INVALID);
                }
                self.path = path as u8;
                self.line = line as u8;
                self.carriage_return = position;
            }
            "TD" => {
                if a.len() > 1 {
                    return Err(INVALID);
                }
                let v = integer(a.first().copied().unwrap_or(0.));
                if v != 0. && v != 1. {
                    return Err(INVALID);
                }
                self.transparent = v == 1.;
            }
            "SS" | "SA" => {
                if !a.is_empty() {
                    return Err(INVALID);
                }
                self.use_alternate = name == "SA";
            }
            "SD" | "AD" => {
                if a.len() % 2 != 0 {
                    return Err(INVALID);
                }
                let font = if name == "SD" {
                    &mut self.standard
                } else {
                    &mut self.alternate
                };
                if a.is_empty() {
                    *font = FontSpec::default();
                }
                for pair in a.chunks_exact(2) {
                    let k = integer(pair[0]);
                    let v = integer(pair[1]);
                    match k as i32 {
                        1 if k == 1. && [14., 21., 277., 341.].contains(&v) => {
                            font.symbol_set = v as u16
                        }
                        2 if k == 2. && v == 0. => font.spacing = 0,
                        3 if k == 3. && pair[1] > 0. && pair[1] <= 32767. => font.pitch = pair[1],
                        4 if k == 4. && (0. ..=32767.).contains(&pair[1]) => {
                            font.height_points = pair[1]
                        }
                        5 if k == 5. && (0. ..=32767.).contains(&v) => font.posture = v as u16,
                        6 if k == 6. && ((-7. ..=7.).contains(&v) || v == 9999.) => {
                            font.weight = v as i16
                        }
                        7 if k == 7. && (0. ..=65535.).contains(&v) => font.typeface = v as u16,
                        _ => return Err(UNSUPPORTED),
                    }
                }
            }
            "SB" => {
                if a.len() > 1 {
                    return Err(INVALID);
                }
                if a.first().copied().unwrap_or(0.) != 0. {
                    return Err(UNSUPPORTED);
                }
            }
            "CF" => {
                if a.len() > 2 {
                    return Err(INVALID);
                }
                if a.first().copied().unwrap_or(0.) != 0. || a.get(1).is_some_and(|v| *v != 0.) {
                    return Err(UNSUPPORTED);
                }
            }
            _ => return Err(UNSUPPORTED),
        }
        Ok(())
    }
    fn font(&self) -> FontSpec {
        if self.use_alternate {
            self.alternate
        } else {
            self.standard
        }
    }
    fn metrics(&self, space: &Space) -> Result<(f64, f64, Point, Point), TransformError> {
        let font = self.font();
        let (w, h) = match self.size_mode {
            "absolute" => (self.width * 10., self.height * 10.),
            "relative" => {
                let e = space.relative_extents_mm()?;
                (self.width * e.x / 100., self.height * e.y / 100.)
            }
            _ => (
                25.4 / font.pitch * 2. / 3.,
                font.height_points * 25.4 / 72. * 2. / 3.,
            ),
        };
        if !w.is_finite() || !h.is_finite() || w.abs() > 1e6 || h.abs() > 1e6 {
            return Err(INVALID);
        }
        let dx = 1.5 * w * (1. + self.extra_space.x);
        let dy = 2. * h * (1. + self.extra_space.y);
        let (advance, feed) = match self.path {
            0 => (Point { x: dx, y: 0. }, Point { x: 0., y: -dy }),
            1 => (Point { x: 0., y: -dy }, Point { x: -dx, y: 0. }),
            2 => (Point { x: -dx, y: 0. }, Point { x: 0., y: dy }),
            _ => (Point { x: 0., y: dy }, Point { x: dx, y: 0. }),
        };
        Ok((
            w,
            h,
            advance,
            mul(feed, if self.line == 0 { 1. } else { -1. }),
        ))
    }
    fn basis(&self, space: &Space) -> Result<(Point, Point), TransformError> {
        let mut d = self.direction;
        if self.relative_direction {
            let e = space.relative_extents_mm()?;
            d.x *= e.x;
            d.y *= e.y;
        }
        let n = length(d);
        if n == 0. || !n.is_finite() {
            return Err(INVALID);
        }
        let angle = d.y.atan2(d.x) + f64::from(space.rotation()).to_radians();
        let u = Point {
            x: angle.cos(),
            y: angle.sin(),
        };
        Ok((u, Point { x: -u.y, y: u.x }))
    }
    pub(crate) fn cp(
        &mut self,
        a: &[f64],
        position: Point,
        space: &Space,
    ) -> Result<Point, TransformError> {
        if !matches!(a.len(), 0 | 2) || a.iter().any(|v| !v.is_finite() || v.abs() > 32767.) {
            return Err(INVALID);
        }
        let (_, _, advance, feed) = self.metrics(space)?;
        let (u, v) = self.basis(space)?;
        let world = |p: Point| add(mul(u, p.x), mul(v, p.y));
        let result = if a.is_empty() {
            self.carriage_return = add(self.carriage_return, world(feed));
            self.carriage_return
        } else {
            let delta = world(mul(feed, -a[1]));
            self.carriage_return = add(self.carriage_return, delta);
            add(position, add(world(mul(advance, a[0])), delta))
        };
        if !physical_ok(result) || !physical_ok(self.carriage_return) {
            return Err(("COORDINATE_RANGE", "Text movement exceeds profile range"));
        }
        Ok(result)
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn label(
        &mut self,
        data: &[u8],
        span: SourceSpan,
        terminator: u8,
        print_terminator: bool,
        position: Point,
        space: &Space,
        max_glyphs: usize,
    ) -> Result<Label, TransformError> {
        let before = self.clone();
        let (u, v) = self.basis(space)?;
        let world = |p: Point| add(position, add(mul(u, p.x), mul(v, p.y)));
        let project = |p: Point| {
            let p = sub(p, position);
            Point {
                x: p.x * u.x + p.y * u.y,
                y: p.x * v.x + p.y * v.y,
            }
        };
        let mut cursor = Point::default();
        let mut cr = project(self.carriage_return);
        let mut line_start = cursor;
        let mut glyphs = Vec::new();
        let mut pending: Vec<(char, usize, Point, f64, f64, FontSpec)> = Vec::new();
        let mut decoded = String::new();
        let mut unmapped = Vec::new();
        let mut history = Vec::new();
        let origin = if self.origin > 10 {
            self.origin - 10
        } else {
            self.origin
        };
        let concatenates = match self.path {
            0 => [1, 2, 3].contains(&origin),
            1 => [3, 6, 9].contains(&origin),
            2 => [7, 8, 9].contains(&origin),
            _ => [1, 4, 7].contains(&origin),
        };
        let flush = |pending: &mut Vec<(char, usize, Point, f64, f64, FontSpec)>,
                     glyphs: &mut Vec<Glyph>,
                     anchor: Point|
         -> Result<(), TransformError> {
            if pending.is_empty() {
                return Ok(());
            }
            // Include leading tab travel; signed SI/SR mirrors the anchor too.
            let mut min = anchor;
            let mut max = anchor;
            for (_, _, p, w, h, _) in pending.iter() {
                for q in [*p, add(*p, Point { x: *w, y: *h })] {
                    min.x = min.x.min(q.x);
                    min.y = min.y.min(q.y);
                    max.x = max.x.max(q.x);
                    max.y = max.y.max(q.y);
                }
            }
            let col = (origin - 1) / 3;
            let row = (origin - 1) % 3;
            let col_fraction = f64::from(if pending[0].3 < 0. { 2 - col } else { col }) / 2.;
            let row_fraction = f64::from(if pending[0].4 < 0. { 2 - row } else { row }) / 2.;
            let mut shift = Point {
                x: anchor.x - (min.x + (max.x - min.x) * col_fraction),
                y: anchor.y - (min.y + (max.y - min.y) * row_fraction),
            };
            if before.origin > 10 {
                let h = pending[0].4.abs() / 2.;
                shift.x += match col {
                    0 => h,
                    2 => -h,
                    _ => 0.,
                };
                shift.y += match row {
                    0 => h,
                    2 => -h,
                    _ => 0.,
                };
            }
            for (ch, offset, p, w, h, font) in pending.drain(..) {
                let origin = world(add(p, shift));
                let axis_x = mul(u, w);
                let axis_y = add(mul(v, h), mul(u, before.slant * h));
                let base = add(origin, mul(axis_y, -0.5));
                let top = add(origin, mul(axis_y, 1.5));
                let envelope = [base, add(base, axis_x), add(top, axis_x), top];
                if !physical_ok(origin) || envelope.iter().any(|p| !physical_ok(*p)) {
                    return Err(("COORDINATE_RANGE", "Label envelope exceeds profile range"));
                }
                glyphs.push(Glyph {
                    character: ch,
                    source_offset: offset,
                    origin,
                    axis_x,
                    axis_y,
                    font,
                    envelope,
                });
            }
            Ok(())
        };
        for (i, byte) in data
            .iter()
            .copied()
            .chain(print_terminator.then_some(terminator))
            .enumerate()
        {
            let offset = span.start + i;
            if !self.transparent && (byte < 32 || (128..160).contains(&byte)) {
                decoded.push(char::from(byte));
                let (_, _, advance, feed) = self.metrics(space)?;
                match byte {
                    8 => {
                        if let Some(p) = history.pop() {
                            cursor = p;
                        }
                    }
                    9 => {
                        let n = length(advance);
                        if n > 0. {
                            let d = sub(cursor, cr);
                            let column = (d.x * advance.x + d.y * advance.y) / (n * n);
                            cursor = add(
                                cursor,
                                mul(advance, ((column / 8.).floor() + 1.) * 8. - column),
                            );
                        }
                    }
                    10 | 13 => {
                        flush(&mut pending, &mut glyphs, line_start)?;
                        if !concatenates {
                            cursor = line_start;
                        }
                        if byte == 10 {
                            cursor = add(cursor, feed);
                            cr = add(cr, feed);
                        } else {
                            cursor = cr;
                        }
                        line_start = cursor;
                        history.clear();
                    }
                    14 => self.use_alternate = true,
                    15 => self.use_alternate = false,
                    _ => {}
                }
                continue;
            }
            let (ch, known) = decode(byte, self.font().symbol_set);
            decoded.push(ch);
            if !known {
                unmapped.push(offset);
            }
            if glyphs.len() + pending.len() >= max_glyphs {
                return Err(("LIMIT_EXCEEDED", "Label glyph limit exceeded"));
            }
            let (w, h, advance, _) = self.metrics(space)?;
            pending.push((ch, offset, cursor, w, h, self.font()));
            history.push(cursor);
            cursor = add(cursor, advance);
            if !physical_ok(world(cursor)) {
                return Err(("COORDINATE_RANGE", "Label cursor exceeds profile range"));
            }
        }
        flush(&mut pending, &mut glyphs, line_start)?;
        if !concatenates {
            cursor = line_start;
        }
        self.carriage_return = world(cr);
        let end = world(cursor);
        if !physical_ok(end) || !physical_ok(self.carriage_return) {
            return Err(("COORDINATE_RANGE", "Label position exceeds profile range"));
        }
        Ok(Label {
            data: data.to_vec(),
            payload_span: span,
            terminator,
            print_terminator,
            decoded,
            glyphs,
            unmapped_offsets: unmapped,
            state: before,
            start: position,
            end,
            approximate: true,
        })
    }
}

fn decode(byte: u8, symbol: u16) -> (char, bool) {
    if (32..127).contains(&byte) {
        return (char::from(byte), true);
    }
    if byte == 255 && symbol == 277 {
        return (' ', true);
    }
    if byte < 32 || byte == 127 {
        return ('\u{fffd}', false);
    }
    let ch = match symbol {
        14 => char::from(byte),
        277 => ROMAN8[(byte - 128) as usize],
        341 => PC8[(byte - 128) as usize],
        _ => '\u{fffd}',
    };
    if ch.is_control() {
        ('\u{fffd}', false)
    } else {
        (ch, ch != '\u{fffd}')
    }
}

// Standard single-byte character mapping; unknown/control glyphs are explicit.
const ROMAN8: [char; 128] = [
    '\u{fffd}', '\u{fffd}', '\u{fffd}', '\u{fffd}', '\u{fffd}', '\u{fffd}', '\u{fffd}', '\u{fffd}',
    '\u{fffd}', '\u{fffd}', '\u{fffd}', '\u{fffd}', '\u{fffd}', '\u{fffd}', '\u{fffd}', '\u{fffd}',
    '\u{fffd}', '\u{fffd}', '\u{fffd}', '\u{fffd}', '\u{fffd}', '\u{fffd}', '\u{fffd}', '\u{fffd}',
    '\u{fffd}', '\u{fffd}', '\u{fffd}', '\u{fffd}', '\u{fffd}', '\u{fffd}', '\u{fffd}', '\u{fffd}',
    '\u{a0}', '\u{c0}', '\u{c2}', '\u{c8}', '\u{ca}', '\u{cb}', '\u{ce}', '\u{cf}', '\u{b4}',
    '\u{2cb}', '\u{2c6}', '\u{a8}', '\u{2dc}', '\u{d9}', '\u{db}', '\u{20a4}', '\u{af}', '\u{dd}',
    '\u{fd}', '\u{b0}', '\u{c7}', '\u{e7}', '\u{d1}', '\u{f1}', '\u{a1}', '\u{bf}', '\u{a4}',
    '\u{a3}', '\u{a5}', '\u{a7}', '\u{192}', '\u{a2}', '\u{e2}', '\u{ea}', '\u{f4}', '\u{fb}',
    '\u{e1}', '\u{e9}', '\u{f3}', '\u{fa}', '\u{e0}', '\u{e8}', '\u{f2}', '\u{f9}', '\u{e4}',
    '\u{eb}', '\u{f6}', '\u{fc}', '\u{c5}', '\u{ee}', '\u{d8}', '\u{c6}', '\u{e5}', '\u{ed}',
    '\u{f8}', '\u{e6}', '\u{c4}', '\u{ec}', '\u{d6}', '\u{dc}', '\u{c9}', '\u{ef}', '\u{df}',
    '\u{d4}', '\u{c1}', '\u{c3}', '\u{e3}', '\u{d0}', '\u{f0}', '\u{cd}', '\u{cc}', '\u{d3}',
    '\u{d2}', '\u{d5}', '\u{f5}', '\u{160}', '\u{161}', '\u{da}', '\u{178}', '\u{ff}', '\u{de}',
    '\u{fe}', '\u{b7}', '\u{b5}', '\u{b6}', '\u{be}', '\u{2014}', '\u{bc}', '\u{bd}', '\u{aa}',
    '\u{ba}', '\u{ab}', '\u{25a0}', '\u{bb}', '\u{b1}', '\u{fffd}',
];

// Standard single-byte character mapping; unknown/control glyphs are explicit.
const PC8: [char; 128] = [
    '\u{c7}', '\u{fc}', '\u{e9}', '\u{e2}', '\u{e4}', '\u{e0}', '\u{e5}', '\u{e7}', '\u{ea}',
    '\u{eb}', '\u{e8}', '\u{ef}', '\u{ee}', '\u{ec}', '\u{c4}', '\u{c5}', '\u{c9}', '\u{e6}',
    '\u{c6}', '\u{f4}', '\u{f6}', '\u{f2}', '\u{fb}', '\u{f9}', '\u{ff}', '\u{d6}', '\u{dc}',
    '\u{a2}', '\u{a3}', '\u{a5}', '\u{20a7}', '\u{192}', '\u{e1}', '\u{ed}', '\u{f3}', '\u{fa}',
    '\u{f1}', '\u{d1}', '\u{aa}', '\u{ba}', '\u{bf}', '\u{2310}', '\u{ac}', '\u{bd}', '\u{bc}',
    '\u{a1}', '\u{ab}', '\u{bb}', '\u{2591}', '\u{2592}', '\u{2593}', '\u{2502}', '\u{2524}',
    '\u{2561}', '\u{2562}', '\u{2556}', '\u{2555}', '\u{2563}', '\u{2551}', '\u{2557}', '\u{255d}',
    '\u{255c}', '\u{255b}', '\u{2510}', '\u{2514}', '\u{2534}', '\u{252c}', '\u{251c}', '\u{2500}',
    '\u{253c}', '\u{255e}', '\u{255f}', '\u{255a}', '\u{2554}', '\u{2569}', '\u{2566}', '\u{2560}',
    '\u{2550}', '\u{256c}', '\u{2567}', '\u{2568}', '\u{2564}', '\u{2565}', '\u{2559}', '\u{2558}',
    '\u{2552}', '\u{2553}', '\u{256b}', '\u{256a}', '\u{2518}', '\u{250c}', '\u{2588}', '\u{2584}',
    '\u{258c}', '\u{2590}', '\u{2580}', '\u{3b1}', '\u{df}', '\u{393}', '\u{3c0}', '\u{3a3}',
    '\u{3c3}', '\u{b5}', '\u{3c4}', '\u{3a6}', '\u{398}', '\u{3a9}', '\u{3b4}', '\u{221e}',
    '\u{3c6}', '\u{3b5}', '\u{2229}', '\u{2261}', '\u{b1}', '\u{2265}', '\u{2264}', '\u{2320}',
    '\u{2321}', '\u{f7}', '\u{2248}', '\u{b0}', '\u{2219}', '\u{b7}', '\u{221a}', '\u{207f}',
    '\u{b2}', '\u{25a0}', '\u{a0}',
];
