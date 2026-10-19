use crate::attributes::Attributes;
use crate::transform::{Space, TransformError};
use crate::{Contour, ContourEdge, Curve, FillStyle, Point, Provenance};

#[derive(Clone, Copy, Default)]
pub(crate) struct Cursor {
    pub active: bool,
    pub first: Option<Point>,
    pub last: Option<Point>,
    pub current_points: usize,
    pub vertices: usize,
    pub contours: usize,
    pub approximate: bool,
}
pub(crate) struct Append {
    pub new_contour: bool,
    pub points: Vec<Point>,
    pub curve: Option<Curve>,
    pub down: bool,
    pub source: Provenance,
}
impl Cursor {
    pub fn append(
        &mut self,
        mut points: Vec<Point>,
        curve: Option<Curve>,
        down: bool,
        skip_start: bool,
        source: Provenance,
        limits: (usize, usize),
    ) -> Result<Append, TransformError> {
        let new_contour = self.first.is_none();
        if new_contour && skip_start && points.len() > 1 {
            points.remove(0);
        }
        let count = points.len() - usize::from(!new_contour);
        if self.vertices.saturating_add(count) > limits.0
            || self.contours + usize::from(new_contour) > limits.1
        {
            return Err(("LIMIT_EXCEEDED", "Polygon buffer limit exceeded"));
        }
        if new_contour {
            self.first = points.first().copied();
            self.current_points = points.len();
            self.contours += 1;
        } else {
            self.current_points += count;
        }
        self.last = points.last().copied();
        self.vertices += count;
        Ok(Append {
            new_contour,
            points,
            curve,
            down,
            source,
        })
    }
}
impl Append {
    pub fn apply(self, contours: &mut Vec<Contour>) {
        if self.new_contour {
            contours.push(Contour {
                points: Vec::new(),
                edges: Vec::new(),
            });
        }
        let c = contours.last_mut().unwrap();
        let start = c.points.len().saturating_sub(1);
        c.points
            .extend(self.points.into_iter().skip(usize::from(!self.new_contour)));
        let end = c.points.len() - 1;
        c.edges.push(ContourEdge {
            start,
            end,
            pen_down: self.down,
            curve: self.curve,
            source: self.source,
        });
    }
}

/// Scanline intersections in physical mm. Half-open crossings avoid double counts
/// at vertices; signed crossings preserve nonzero winding and opposite holes.
pub(crate) fn fill(
    contours: &[Contour],
    attrs: &Attributes,
    space: &Space,
    rule: u8,
    max_lines: usize,
) -> Result<FillStyle, TransformError> {
    let mut result = FillStyle {
        fill_type: attrs.fill_type,
        rule: if rule == 0 { "evenodd" } else { "nonzero" },
        spacing_mm: None,
        angle_deg: 0.,
        hatch_lines: Vec::new(),
        hatch_phases_mm: Vec::new(),
    };
    if attrs.fill_type < 3 {
        return Ok(result);
    }
    let h = attrs.hatch[(attrs.fill_type - 3) as usize];
    let pattern = if attrs.line.kind.is_some_and(|k| k > 0) {
        attrs.pattern_length(space)?
    } else {
        None
    };
    let spacing = if h.spacing == 0. {
        space.diagonal_mm()? / 100.
    } else if h.user_units {
        h.spacing * space.x_unit_mm()?
    } else {
        h.spacing / 40.
    };
    if !spacing.is_finite() || !(1e-6..=1e6).contains(&spacing) {
        return Err(("INVALID_ARGUMENTS", "Invalid hatch spacing"));
    }
    let angle = h.angle + f64::from(space.rotation());
    result.spacing_mm = Some(spacing);
    result.angle_deg = angle;
    // The default anchor is the rotated plotter origin, independent of SC.
    let mut anchor = space.clone();
    anchor.command("SC", &[])?;
    let origin = crate::transform::mm(anchor.map(Point::default(), false)?);
    let edge_count: usize = contours
        .iter()
        .map(|c| c.points.len().saturating_sub(1))
        .sum();
    for angle in [Some(angle), (attrs.fill_type == 4).then_some(angle + 90.)]
        .into_iter()
        .flatten()
    {
        let cos = angle.to_radians().cos();
        let sin = angle.to_radians().sin();
        let project = |p: Point| Point {
            x: (p.x - origin.x) * cos + (p.y - origin.y) * sin,
            y: -(p.x - origin.x) * sin + (p.y - origin.y) * cos,
        };
        let unproject = |x: f64, y: f64| Point {
            x: origin.x + x * cos - y * sin,
            y: origin.y + x * sin + y * cos,
        };
        let mut lo = f64::INFINITY;
        let mut hi = f64::NEG_INFINITY;
        for c in contours.iter().filter(|c| c.points.len() >= 3) {
            for &p in &c.points {
                let y = project(p).y;
                lo = lo.min(y);
                hi = hi.max(y);
            }
        }
        if !lo.is_finite() {
            continue;
        }
        let first = (lo / spacing).ceil();
        let last = (hi / spacing).floor();
        let count = (last - first + 1.).max(0.);
        if count > max_lines as f64 || count * edge_count as f64 > 4_000_000. {
            return Err(("LIMIT_EXCEEDED", "Hatch intersection work limit exceeded"));
        }
        for row in 0..count as usize {
            let y = (first + row as f64) * spacing;
            let mut crossings = Vec::new();
            for c in contours.iter().filter(|c| c.points.len() >= 3) {
                for pair in c.points.windows(2) {
                    let a = project(pair[0]);
                    let b = project(pair[1]);
                    if (a.y <= y && y < b.y) || (b.y <= y && y < a.y) {
                        let x = a.x + (y - a.y) * (b.x - a.x) / (b.y - a.y);
                        crossings.push((x, if b.y > a.y { 1 } else { -1 }));
                    }
                }
            }
            crossings.sort_by(|a, b| a.0.total_cmp(&b.0));
            let mut winding = 0i64;
            let mut start = 0.;
            let mut index = 0;
            while index < crossings.len() {
                let x = crossings[index].0;
                let before = if rule == 0 {
                    winding % 2 != 0
                } else {
                    winding != 0
                };
                while index < crossings.len() && crossings[index].0 == x {
                    winding += if rule == 0 { 1 } else { crossings[index].1 };
                    index += 1;
                }
                let after = if rule == 0 {
                    winding % 2 != 0
                } else {
                    winding != 0
                };
                if !before && after {
                    start = x;
                }
                if before && !after && x > start {
                    if result.hatch_lines.len() >= max_lines {
                        return Err(("LIMIT_EXCEEDED", "Hatch line limit exceeded"));
                    }
                    result
                        .hatch_lines
                        .push([unproject(start, y), unproject(x, y)]);
                    if let Some(period) = pattern {
                        let stagger = (first + row as f64).rem_euclid(2.) * period / 2.;
                        result
                            .hatch_phases_mm
                            .push((start + stagger).rem_euclid(period));
                    }
                }
            }
        }
    }
    Ok(result)
}
