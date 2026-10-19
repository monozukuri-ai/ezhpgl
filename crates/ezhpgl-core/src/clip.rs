use crate::{Bounds, ClipRegion, Geometry, PlotOperation, Point};

fn expand(b: Bounds, amount: f64) -> Bounds {
    Bounds {
        min: Point {
            x: b.min.x - amount,
            y: b.min.y - amount,
        },
        max: Point {
            x: b.max.x + amount,
            y: b.max.y + amount,
        },
    }
}

pub(crate) fn include(bounds: &mut Option<Bounds>, point: Point) {
    match bounds {
        Some(b) => b.include(point),
        None => {
            *bounds = Some(Bounds {
                min: point,
                max: point,
            })
        }
    }
}

/// Liang-Barsky clipping of a centerline; never updates the interpreter's cursor.
fn segment(a: Point, b: Point, clip: ClipRegion) -> Option<(Point, Point)> {
    let window = clip.bounds()?;
    if window.min.x >= window.max.x || window.min.y >= window.max.y {
        return None;
    }
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    let mut enter: f64 = 0.;
    let mut leave: f64 = 1.;
    for (p, q) in [
        (-dx, a.x - window.min.x),
        (dx, window.max.x - a.x),
        (-dy, a.y - window.min.y),
        (dy, window.max.y - a.y),
    ] {
        if p == 0. {
            if q < 0. {
                return None;
            }
        } else {
            let t = q / p;
            if p < 0. {
                enter = enter.max(t);
            } else {
                leave = leave.min(t);
            }
            if enter > leave {
                return None;
            }
        }
    }
    Some((
        Point {
            x: a.x + enter * dx,
            y: a.y + enter * dy,
        },
        Point {
            x: a.x + leave * dx,
            y: a.y + leave * dy,
        },
    ))
}

impl PlotOperation {
    pub fn visible_bounds(&self) -> Option<Bounds> {
        if matches!(self.geometry, Geometry::Label { .. }) {
            let mut result = None;
            for &p in self.geometry.all_points() {
                include(&mut result, p);
            }
            return result.and_then(|b| match self.clip {
                None => Some(b),
                Some(clip) => ClipRegion::Rect {
                    min: b.min,
                    max: b.max,
                }
                .intersect(clip)
                .bounds(),
            });
        }
        if let Geometry::Polygon { contours, fill } = &self.geometry {
            let mut result = None;
            if fill.as_ref().is_some_and(|f| f.fill_type < 3) {
                for c in contours.iter().filter(|c| c.points.len() >= 3) {
                    for &p in &c.points {
                        include(&mut result, p);
                    }
                }
                return result.and_then(|b| match self.clip {
                    None => Some(b),
                    Some(clip) => ClipRegion::Rect {
                        min: b.min,
                        max: b.max,
                    }
                    .intersect(clip)
                    .bounds(),
                });
            }
            let mut add_path = |points: &[Point]| {
                for pair in points.windows(2) {
                    let visible = self
                        .clip
                        .map_or(Some((pair[0], pair[1])), |c| segment(pair[0], pair[1], c));
                    if let Some((a, b)) = visible {
                        include(&mut result, a);
                        include(&mut result, b);
                    }
                }
            };
            if let Some(fill) = fill {
                for line in &fill.hatch_lines {
                    add_path(line);
                }
            } else {
                for c in contours.iter().filter(|c| c.points.len() >= 3) {
                    for e in c.edges.iter().filter(|e| e.pen_down) {
                        if let Some(points) = c.points.get(e.start..=e.end) {
                            add_path(points);
                        }
                    }
                }
            }
            return result;
        }
        let points = self.geometry.points();
        let Some(clip) = self.clip else {
            let mut result = None;
            for &p in points {
                include(&mut result, p);
            }
            return result;
        };
        let mut result = None;
        if self.geometry.start_dot() {
            if let Some(&p) = points.first() {
                if segment(p, p, clip).is_some() {
                    include(&mut result, p);
                }
            }
        }
        for pair in points.windows(2) {
            if let Some((a, b)) = segment(pair[0], pair[1], clip) {
                include(&mut result, a);
                include(&mut result, b);
            }
        }
        result
    }

    /// Conservative painted bounds, including strokes whose centers lie outside IW.
    pub(crate) fn paint_bounds(&self) -> Option<Bounds> {
        let mut result = None;
        for &p in self.geometry.all_points() {
            include(&mut result, p);
        }
        let factor = self
            .style
            .line
            .as_ref()
            .map_or(2.5, |l| (l.miter_limit / 2.).max(1.));
        // Also accommodates degenerate fills rendered as a dot or line.
        let b = expand(
            result?,
            if matches!(self.geometry, Geometry::Label { .. }) {
                0.
            } else {
                self.style.width_mm * factor
            },
        );
        match self.clip {
            None => Some(b),
            Some(clip) => ClipRegion::Rect {
                min: b.min,
                max: b.max,
            }
            .intersect(clip)
            .bounds(),
        }
    }
}
