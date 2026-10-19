use crate::transform::{Space, TransformError};
use crate::{LineStyle, Point, Style};
use std::{collections::BTreeMap, sync::Arc};

const INVALID: TransformError = ("INVALID_ARGUMENTS", "Invalid line/fill attribute arguments");
const UNSUPPORTED: TransformError = (
    "UNSUPPORTED_ATTRIBUTE",
    "This attribute variant is outside the preview profile",
);

#[derive(Clone, Copy, Debug)]
pub(crate) struct LineSpec {
    pub kind: Option<i8>,
    pub length: f64,
    pub relative: bool,
}
impl Default for LineSpec {
    fn default() -> Self {
        Self {
            kind: None,
            length: 4.,
            relative: true,
        }
    }
}

#[derive(Clone, Copy, Default)]
pub(crate) struct HatchSpec {
    pub spacing: f64,
    pub angle: f64,
    pub user_units: bool,
}

#[derive(Clone)]
pub(crate) struct Attributes {
    pub palette_size: Option<u32>,
    colors: Arc<BTreeMap<u32, String>>,
    widths: Arc<BTreeMap<u32, f64>>,
    width: f64,
    relative_width: bool,
    pub line: LineSpec,
    pub phase: f64,
    pub last_end: Option<Point>,
    saved: Option<(LineSpec, f64, Point)>,
    cap: u8,
    join: u8,
    miter: f64,
    pub fill_type: u8,
    pub hatch: [HatchSpec; 2],
    // CR black/white reference values per RGB component; PC values are mapped to 0..255
    color_range: [f64; 6],
}
impl Default for Attributes {
    fn default() -> Self {
        Self {
            palette_size: None,
            colors: Arc::default(),
            widths: Arc::default(),
            width: 0.35,
            relative_width: false,
            color_range: [0., 255., 0., 255., 0., 255.],
            line: LineSpec::default(),
            phase: 0.,
            last_end: None,
            saved: None,
            cap: 1,
            join: 1,
            miter: 5.,
            fill_type: 1,
            hatch: [HatchSpec::default(); 2],
        }
    }
}
impl Attributes {
    pub fn defaults(&mut self) {
        self.line = LineSpec::default();
        self.phase = 0.;
        self.last_end = None;
        self.saved = None;
        self.cap = 1;
        self.join = 1;
        self.miter = 5.;
        self.fill_type = 1;
        self.hatch = [HatchSpec::default(); 2];
    }
    pub fn break_path(&mut self) {
        self.phase = 0.;
        self.last_end = None;
        self.saved = None;
    }
    pub fn begin_stroke(&mut self, start: Point) {
        if self.last_end != Some(start) {
            self.phase = 0.;
        }
    }
    pub fn select(&self, pen: u32) -> u32 {
        if pen == 0 {
            0
        } else {
            self.palette_size.map_or(pen, |n| 1 + (pen - 1) % n)
        }
    }
    pub fn freeze_hatch(&mut self, space: &Space) -> Result<(), TransformError> {
        for hatch in &mut self.hatch {
            if hatch.user_units && hatch.spacing != 0. {
                hatch.spacing *= space.x_unit_mm()? * 40.;
                hatch.user_units = false;
            }
        }
        Ok(())
    }
    pub fn pattern_length(&self, space: &Space) -> Result<Option<f64>, TransformError> {
        match self.line.kind {
            Some(k) if k != 0 => {
                let v = if self.line.relative {
                    self.line.length * space.diagonal_mm()? / 100.
                } else {
                    self.line.length
                };
                if !v.is_finite() || !(1e-6..=1e6).contains(&v) {
                    return Err(INVALID);
                }
                Ok(Some(v))
            }
            _ => Ok(None),
        }
    }
    pub fn snapshot(&self, pen: u32, space: &Space) -> Result<Style, TransformError> {
        let width = *self.widths.get(&pen).unwrap_or(&self.width);
        let width = if self.relative_width {
            width * space.diagonal_mm()? / 100.
        } else {
            width
        };
        if !width.is_finite() || width > 1e6 {
            return Err(INVALID);
        }
        let pattern_length_mm = self.pattern_length(space)?;
        let line =
            if self.line.kind.is_none() && self.cap == 1 && self.join == 1 && self.miter == 5. {
                None
            } else {
                Some(LineStyle {
                    line_type: self.line.kind,
                    pattern_length_mm,
                    phase_mm: self.phase,
                    cap: self.cap,
                    join: self.join,
                    miter_limit: self.miter,
                })
            };
        Ok(Style {
            pen,
            color: self
                .colors
                .get(&pen)
                .cloned()
                .unwrap_or_else(|| "#000000".into()),
            width_mm: width.max(0.025),
            line,
        })
    }
    pub fn command(
        &mut self,
        name: &str,
        a: &[f64],
        space: &Space,
        position: Point,
        pen: &mut u32,
    ) -> Result<(), TransformError> {
        if a.iter().any(|v| !v.is_finite() || v.abs() > 1_073_741_824.) {
            return Err(INVALID);
        }
        let int = |v: f64| (v + 0.5).floor();
        match name {
            "NP" => {
                if a.len() > 1 {
                    return Err(INVALID);
                }
                let n = int(a.first().copied().unwrap_or(8.));
                if n < 2. {
                    return Err(INVALID);
                }
                let n = (n.min(256.) as u32).next_power_of_two();
                Arc::make_mut(&mut self.colors).retain(|k, _| *k <= n);
                Arc::make_mut(&mut self.widths).retain(|k, _| *k <= n);
                self.palette_size = Some(n);
                *pen = self.select(*pen);
            }
            "PC" => {
                if !matches!(a.len(), 0 | 1 | 4) {
                    return Err(INVALID);
                }
                if a.is_empty() {
                    self.colors = Arc::default();
                } else {
                    let id = self.pen_id(a[0])?;
                    if a.len() == 1 {
                        Arc::make_mut(&mut self.colors).remove(&id);
                    } else {
                        if !self.colors.contains_key(&id) && self.colors.len() >= 256 {
                            return Err(("LIMIT_EXCEEDED", "Pen color entry limit exceeded"));
                        }
                        let rgb = a[1..]
                            .iter()
                            .enumerate()
                            .map(|(i, v)| {
                                let (black, white) =
                                    (self.color_range[2 * i], self.color_range[2 * i + 1]);
                                let scaled = (v - black) / (white - black) * 255.;
                                int(scaled.clamp(0., 255.)) as u8
                            })
                            .collect::<Vec<_>>();
                        Arc::make_mut(&mut self.colors)
                            .insert(id, format!("#{:02x}{:02x}{:02x}", rgb[0], rgb[1], rgb[2]));
                    }
                }
            }
            "WU" => {
                if a.len() > 1 {
                    return Err(INVALID);
                }
                let mode = int(a.first().copied().unwrap_or(0.));
                if mode != 0. && mode != 1. {
                    return Err(INVALID);
                }
                self.relative_width = mode == 1.;
                self.width = if self.relative_width { 0.1 } else { 0.35 };
                self.widths = Arc::default();
            }
            "PW" => {
                if a.len() > 2 {
                    return Err(INVALID);
                }
                let width =
                    a.first()
                        .copied()
                        .unwrap_or(if self.relative_width { 0.1 } else { 0.35 });
                if !(0. ..=32767.).contains(&width) {
                    return Err(INVALID);
                }
                if a.len() == 2 {
                    let id = self.pen_id(a[1])?;
                    if !self.widths.contains_key(&id) && self.widths.len() >= 256 {
                        return Err(("LIMIT_EXCEEDED", "Pen width entry limit exceeded"));
                    }
                    Arc::make_mut(&mut self.widths).insert(id, width);
                } else {
                    self.width = width;
                    self.widths = Arc::default();
                }
            }
            "LA" => {
                if a.len() % 2 != 0 {
                    return Err(INVALID);
                }
                if a.is_empty() {
                    self.cap = 1;
                    self.join = 1;
                    self.miter = 5.;
                }
                for pair in a.chunks_exact(2) {
                    let kind = int(pair[0]);
                    let value = int(pair[1]);
                    match kind as i32 {
                        1 if kind == 1. && [1., 2., 4.].contains(&value) => self.cap = value as u8,
                        2 if kind == 2. && [1., 2., 4., 5., 6.].contains(&value) => {
                            self.join = value as u8
                        }
                        3 if kind == 3. => self.miter = pair[1].clamp(1., 32767.),
                        _ => return Err(UNSUPPORTED),
                    }
                }
            }
            "LT" => {
                if a.len() > 3 {
                    return Err(INVALID);
                }
                if a.is_empty() {
                    if self.line.kind.is_some() {
                        self.saved = Some((self.line, self.phase, position));
                    }
                    self.line.kind = None;
                    self.phase = 0.;
                    return Ok(());
                }
                let kind = int(a[0]);
                if kind == 99. {
                    if a.len() != 1 {
                        return Err(INVALID);
                    }
                    if self.line.kind.is_none() {
                        if let Some((line, phase, at)) =
                            self.saved.filter(|(_, _, p)| *p == position)
                        {
                            self.line = line;
                            self.phase = phase;
                            self.last_end = Some(crate::transform::mm(position));
                            self.saved = Some((line, phase, at));
                        }
                    }
                    return Ok(());
                }
                if !(-8. ..=8.).contains(&kind) {
                    return Err(INVALID);
                }
                self.line.kind = Some(kind as i8);
                if a.len() >= 2 {
                    if a[1] <= 0. {
                        return Err(INVALID);
                    }
                    self.line.length = a[1].min(32767.);
                }
                if a.len() == 3 {
                    let mode = int(a[2]);
                    if mode != 0. && mode != 1. {
                        return Err(INVALID);
                    }
                    self.line.relative = mode == 0.;
                }
            }
            "FT" => {
                if a.len() > 3 {
                    return Err(INVALID);
                }
                let kind = int(a.first().copied().unwrap_or(1.));
                if ![1., 2., 3., 4.].contains(&kind) {
                    return Err(UNSUPPORTED);
                }
                self.fill_type = kind as u8;
                if kind >= 3. {
                    let h = &mut self.hatch[(kind as usize) - 3];
                    if a.len() >= 2 {
                        if a[1] < 0. {
                            return Err(INVALID);
                        }
                        h.spacing = a[1].min(32767.);
                        h.user_units = space.is_scaled();
                    }
                    if a.len() == 3 {
                        h.angle = a[2].clamp(-32768., 32767.) % 360.;
                    }
                }
            }
            "CR" => {
                if !matches!(a.len(), 0 | 2 | 6) {
                    return Err(INVALID);
                }
                let ranges: Vec<f64> = match a.len() {
                    0 => vec![0., 255., 0., 255., 0., 255.],
                    2 => vec![a[0], a[1], a[0], a[1], a[0], a[1]],
                    _ => a.to_vec(),
                };
                if ranges.chunks(2).any(|pair| pair[0] == pair[1]) {
                    return Err(INVALID);
                }
                self.color_range = [
                    ranges[0], ranges[1], ranges[2], ranges[3], ranges[4], ranges[5],
                ];
            }
            _ => return Err(INVALID),
        }
        if !matches!(name, "FT" | "PC" | "NP" | "CR") {
            self.break_path();
        }
        Ok(())
    }
    fn pen_id(&self, v: f64) -> Result<u32, TransformError> {
        if !(0. ..=8_388_607.).contains(&v) {
            return Err(INVALID);
        }
        let id = v.round() as u32;
        if self.palette_size.is_some_and(|n| id > n) {
            return Err(INVALID);
        }
        Ok(id)
    }
}

pub(crate) fn pattern(kind: i8) -> &'static [f64] {
    match kind.abs() {
        1 => &[0., 100.],
        2 => &[50., 50.],
        3 => &[70., 30.],
        4 => &[80., 10., 0., 10.],
        5 => &[70., 10., 10., 10.],
        6 => &[50., 10., 10., 10., 10., 10.],
        7 => &[70., 10., 0., 10., 0., 10.],
        8 => &[50., 10., 0., 10., 10., 10., 0., 10.],
        _ => &[],
    }
}
