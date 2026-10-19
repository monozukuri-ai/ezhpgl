use serde::Serialize;

use crate::{Bounds, Point};

/// Explicit virtual-device defaults. Plot extents are not inferred paper dimensions.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize)]
pub struct DeviceProfile {
    pub plot_size_mm: Option<[f64; 2]>,
    pub paper_size_mm: Option<[f64; 2]>,
}

impl DeviceProfile {
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }
    pub(crate) fn validate(&self) -> bool {
        [self.plot_size_mm, self.paper_size_mm]
            .into_iter()
            .flatten()
            .all(|size| {
                size.iter()
                    .all(|v| v.is_finite() && *v > 0.0 && *v <= 8_388_607.0 / 40.0)
            })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ClipRegion {
    Rect { min: Point, max: Point },
    Empty,
}

impl ClipRegion {
    pub fn bounds(self) -> Option<Bounds> {
        match self {
            Self::Rect { min, max } => Some(Bounds { min, max }),
            Self::Empty => None,
        }
    }
    pub(crate) fn intersect(self, other: Self) -> Self {
        match (self.bounds(), other.bounds()) {
            (Some(a), Some(b)) => {
                let min = Point {
                    x: a.min.x.max(b.min.x),
                    y: a.min.y.max(b.min.y),
                };
                let max = Point {
                    x: a.max.x.min(b.max.x),
                    y: a.max.y.min(b.max.y),
                };
                if min.x >= max.x || min.y >= max.y {
                    Self::Empty
                } else {
                    Self::Rect { min, max }
                }
            }
            _ => Self::Empty,
        }
    }
}

/// Current-user coordinates -> physical mm, using x'=a*x+c*y+e, y'=b*x+d*y+f.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct TransformState {
    pub p1_plotter_units: Point,
    pub p2_plotter_units: Option<Point>,
    pub rotation: u16,
    pub scaling: &'static str,
    pub matrix: [f64; 6],
    pub clip: Option<ClipRegion>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
enum Scaling {
    #[default]
    Off,
    Anisotropic([f64; 4]),
    Isotropic {
        ranges: [f64; 4],
        align: [f64; 2],
    },
    PointFactor([f64; 4]),
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Window {
    Plotter(Bounds),
    User(Bounds),
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Space {
    p1: Point,
    p2: Option<Point>,
    rotation: u16,
    scaling: Scaling,
    window: Option<Window>,
    pub plot_size: Option<[f64; 2]>, // unrotated physical extents, plotter units
}

pub(crate) type TransformError = (&'static str, &'static str);
const MISSING: TransformError = (
    "MISSING_DEVICE_BOUNDS",
    "This operation needs explicit plot extents or scaling points",
);
const INVALID: TransformError = ("INVALID_ARGUMENTS", "Invalid transform command arguments");
const RANGE: TransformError = (
    "TRANSFORM_RANGE",
    "Transform is singular, non-finite or exceeds the supported range",
);

impl Space {
    pub fn relative_extents_mm(&self) -> Result<Point, TransformError> {
        let p2 = self.p2.ok_or(MISSING)?;
        let (x, y) = match self.scaling {
            Scaling::Isotropic { ranges, .. } => {
                let [sx, sy, _, _] = self.scale()?;
                (sx * (ranges[1] - ranges[0]), sy * (ranges[3] - ranges[2]))
            }
            _ => (p2.x - self.p1.x, p2.y - self.p1.y),
        };
        Ok(Point {
            x: x / 40.,
            y: y / 40.,
        })
    }
    pub fn unmap(&self, p: Point) -> Result<Point, TransformError> {
        let origin = self.map(Point::default(), false)?;
        let u = self.map(Point { x: 1., y: 0. }, true)?;
        let v = self.map(Point { x: 0., y: 1. }, true)?;
        let dx = p.x - origin.x;
        let dy = p.y - origin.y;
        Ok(Point {
            x: (dx * u.x + dy * u.y) / (u.x * u.x + u.y * u.y),
            y: (dx * v.x + dy * v.y) / (v.x * v.x + v.y * v.y),
        })
    }
    pub fn diagonal_mm(&self) -> Result<f64, TransformError> {
        let p2 = self.p2.ok_or(MISSING)?;
        let (dx, dy) = match self.scaling {
            Scaling::Isotropic { ranges, .. } => {
                let [sx, sy, _, _] = self.scale()?;
                (sx * (ranges[1] - ranges[0]), sy * (ranges[3] - ranges[2]))
            }
            _ => (p2.x - self.p1.x, p2.y - self.p1.y),
        };
        Ok(dx.hypot(dy) / 40.)
    }
    pub fn x_unit_mm(&self) -> Result<f64, TransformError> {
        Ok(self.scale()?[0].abs() / 40.)
    }
    pub fn is_scaled(&self) -> bool {
        self.scaling != Scaling::Off
    }
    pub fn rotation(&self) -> u16 {
        self.rotation
    }
    pub fn new(profile: DeviceProfile) -> Self {
        let plot_size = profile.plot_size_mm.map(|s| [s[0] * 40.0, s[1] * 40.0]);
        Self {
            p1: Point::default(),
            p2: plot_size.map(|s| Point { x: s[0], y: s[1] }),
            plot_size,
            rotation: 0,
            scaling: Scaling::Off,
            window: None,
        }
    }

    fn oriented_size(&self) -> Option<[f64; 2]> {
        self.plot_size.map(|s| {
            if self.rotation % 180 == 0 {
                s
            } else {
                [s[1], s[0]]
            }
        })
    }

    fn reset_points(&mut self) {
        self.p1 = Point::default();
        self.p2 = self.oriented_size().map(|s| Point { x: s[0], y: s[1] });
    }

    pub fn defaults(&mut self) {
        self.scaling = Scaling::Off;
        self.window = None;
    }

    pub fn initialize(&mut self) {
        self.defaults();
        self.rotation = 0;
        self.reset_points();
    }

    /// Returns diagonal scales and offset in the rotated plotter coordinate system.
    fn scale(&self) -> Result<[f64; 4], TransformError> {
        let result = match self.scaling {
            Scaling::Off => [1., 1., 0., 0.],
            Scaling::PointFactor([xmin, sx, ymin, sy]) => {
                [sx, sy, self.p1.x - xmin * sx, self.p1.y - ymin * sy]
            }
            Scaling::Anisotropic(r) | Scaling::Isotropic { ranges: r, .. } => {
                let p2 = self.p2.ok_or(MISSING)?;
                let dx = p2.x - self.p1.x;
                let dy = p2.y - self.p1.y;
                let mut sx = dx / (r[1] - r[0]);
                let mut sy = dy / (r[3] - r[2]);
                let mut origin = self.p1;
                if let Scaling::Isotropic { align, .. } = self.scaling {
                    let unit = sx.abs().min(sy.abs());
                    sx = unit.copysign(sx);
                    sy = unit.copysign(sy);
                    origin.x += (dx - sx * (r[1] - r[0])) * align[0] / 100.;
                    origin.y += (dy - sy * (r[3] - r[2])) * align[1] / 100.;
                }
                [sx, sy, origin.x - r[0] * sx, origin.y - r[2] * sy]
            }
        };
        if result.iter().any(|v| !v.is_finite() || v.abs() > 1e12)
            || result[0].abs() < 1e-12
            || result[1].abs() < 1e-12
        {
            return Err(RANGE);
        }
        Ok(result)
    }

    fn rotate(&self, p: Point, vector: bool) -> Point {
        let [w, h] = if vector {
            [0., 0.]
        } else {
            self.plot_size.unwrap_or([0., 0.])
        };
        match self.rotation {
            90 => Point { x: w - p.y, y: p.x },
            180 => Point {
                x: w - p.x,
                y: h - p.y,
            },
            270 => Point { x: p.y, y: h - p.x },
            _ => p,
        }
    }

    pub fn map(&self, p: Point, vector: bool) -> Result<Point, TransformError> {
        let [sx, sy, tx, ty] = self.scale()?;
        Ok(self.rotate(
            Point {
                x: p.x * sx + if vector { 0. } else { tx },
                y: p.y * sy + if vector { 0. } else { ty },
            },
            vector,
        ))
    }

    fn unscaled_window(&self, window: Window) -> Result<Bounds, TransformError> {
        match window {
            Window::Plotter(b) => Ok(b),
            Window::User(b) => {
                let [sx, sy, tx, ty] = self.scale()?;
                Ok(rect(
                    Point {
                        x: b.min.x * sx + tx,
                        y: b.min.y * sy + ty,
                    },
                    Point {
                        x: b.max.x * sx + tx,
                        y: b.max.y * sy + ty,
                    },
                ))
            }
        }
    }

    pub fn clip(&self) -> Result<Option<ClipRegion>, TransformError> {
        let hard = self.plot_size.map(|s| ClipRegion::Rect {
            min: Point::default(),
            max: mm(Point { x: s[0], y: s[1] }),
        });
        let soft = if let Some(w) = self.window {
            let b = self.unscaled_window(w)?;
            let b = rect(mm(self.rotate(b.min, false)), mm(self.rotate(b.max, false)));
            if !point_ok_mm(b.min) || !point_ok_mm(b.max) {
                return Err(RANGE);
            }
            Some(ClipRegion::Rect {
                min: b.min,
                max: b.max,
            })
        } else {
            None
        };
        Ok(match (hard, soft) {
            (Some(a), Some(b)) => Some(a.intersect(b)),
            (a, b) => a.or(b),
        })
    }

    pub fn snapshot(&self) -> Result<Option<TransformState>, TransformError> {
        if *self == Self::new(DeviceProfile::default()) {
            return Ok(None);
        }
        let a = mm(self.map(Point { x: 1., y: 0. }, true)?);
        let b = mm(self.map(Point { x: 0., y: 1. }, true)?);
        let c = mm(self.map(Point::default(), false)?);
        Ok(Some(TransformState {
            p1_plotter_units: self.p1,
            p2_plotter_units: self.p2,
            rotation: self.rotation,
            scaling: match self.scaling {
                Scaling::Off => "off",
                Scaling::Anisotropic(_) => "anisotropic",
                Scaling::Isotropic { .. } => "isotropic",
                Scaling::PointFactor(_) => "point_factor",
            },
            matrix: [a.x, a.y, b.x, b.y, c.x, c.y],
            clip: self.clip()?,
        }))
    }

    /// Called on a candidate copy; the caller commits only after validation succeeds.
    pub fn command(&mut self, name: &str, args: &[f64]) -> Result<(), TransformError> {
        if args
            .iter()
            .any(|v| !v.is_finite() || v.abs() > 1_073_741_824.)
        {
            return Err(RANGE);
        }
        match name {
            "IP" | "IR" => {
                if !matches!(args.len(), 0 | 2 | 4) {
                    return Err(INVALID);
                }
                if args.is_empty() {
                    self.reset_points();
                } else {
                    let convert = |x: f64, y: f64| -> Result<Point, TransformError> {
                        if name == "IR" {
                            let [w, h] = self.oriented_size().ok_or(MISSING)?;
                            Ok(Point {
                                x: x.clamp(-32768., 32767.) / 100. * w,
                                y: y.clamp(-32768., 32767.) / 100. * h,
                            })
                        } else {
                            Ok(Point {
                                x: hp_round(x),
                                y: hp_round(y),
                            })
                        }
                    };
                    let p1 = convert(args[0], args[1])?;
                    let mut p2 = if args.len() == 4 {
                        convert(args[2], args[3])?
                    } else {
                        let previous = self.p2.ok_or(MISSING)?;
                        Point {
                            x: previous.x + p1.x - self.p1.x,
                            y: previous.y + p1.y - self.p1.y,
                        }
                    };
                    if p1.x == p2.x {
                        p2.x += 1.;
                    }
                    if p1.y == p2.y {
                        p2.y += 1.;
                    }
                    if !point_ok_mm(mm(p1)) || !point_ok_mm(mm(p2)) {
                        return Err(RANGE);
                    }
                    self.p1 = p1;
                    self.p2 = Some(p2);
                }
            }
            "SC" => {
                if !matches!(args.len(), 0 | 4 | 5 | 7) {
                    return Err(INVALID);
                }
                // SC freezes a user-space window using the PREVIOUS scale.
                if let Some(window) = self.window {
                    self.window = Some(Window::Plotter(self.unscaled_window(window)?));
                }
                self.scaling = if args.is_empty() {
                    Scaling::Off
                } else {
                    let r = [args[0], args[1], args[2], args[3]];
                    let kind = if args.len() >= 5 {
                        hp_round(args[4])
                    } else {
                        0.
                    };
                    match kind as i32 {
                        0 | 1 if kind == 0. || kind == 1. => {
                            if r[0] == r[1] || r[2] == r[3] {
                                return Err(INVALID);
                            }
                            if kind == 0. {
                                Scaling::Anisotropic(r)
                            } else {
                                Scaling::Isotropic {
                                    ranges: r,
                                    align: if args.len() == 7 {
                                        [args[5].clamp(0., 100.), args[6].clamp(0., 100.)]
                                    } else {
                                        [50., 50.]
                                    },
                                }
                            }
                        }
                        2 if kind == 2. && args.len() == 5 && r[1] != 0. && r[3] != 0. => {
                            Scaling::PointFactor(r)
                        }
                        _ => return Err(INVALID),
                    }
                };
            }
            "RO" => {
                if args.len() > 1 {
                    return Err(INVALID);
                }
                let angle = hp_round(args.first().copied().unwrap_or(0.));
                if ![0., 90., 180., 270.].contains(&angle) {
                    return Err(INVALID);
                }
                if angle != 0. && self.plot_size.is_none() {
                    return Err(MISSING);
                }
                self.rotation = angle as u16;
            }
            "IW" => {
                if !matches!(args.len(), 0 | 4) {
                    return Err(INVALID);
                }
                self.window = if args.is_empty() {
                    None
                } else {
                    let b = rect(
                        Point {
                            x: args[0],
                            y: args[1],
                        },
                        Point {
                            x: args[2],
                            y: args[3],
                        },
                    );
                    Some(if self.scaling == Scaling::Off {
                        Window::Plotter(b)
                    } else {
                        Window::User(b)
                    })
                };
            }
            _ => return Err(INVALID),
        }
        self.scale()?;
        self.clip()?;
        Ok(())
    }
}

fn hp_round(value: f64) -> f64 {
    (value + 0.5).floor()
}
pub(crate) fn rect(a: Point, b: Point) -> Bounds {
    Bounds {
        min: Point {
            x: a.x.min(b.x),
            y: a.y.min(b.y),
        },
        max: Point {
            x: a.x.max(b.x),
            y: a.y.max(b.y),
        },
    }
}
pub(crate) fn mm(p: Point) -> Point {
    Point {
        x: p.x / 40.,
        y: p.y / 40.,
    }
}
pub(crate) fn point_ok_mm(p: Point) -> bool {
    p.x.is_finite() && p.y.is_finite() && p.x.abs() <= 1e9 && p.y.abs() <= 1e9
}
