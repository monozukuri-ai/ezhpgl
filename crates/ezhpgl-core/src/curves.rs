use crate::transform::{mm, Space, TransformError};
use crate::{ChordTolerance, Curve, Geometry, Point};

pub(crate) const INVALID: TransformError = ("INVALID_ARGUMENTS", "Invalid curve arguments");
pub(crate) const LIMIT: TransformError = ("LIMIT_EXCEEDED", "Curve subdivision limit exceeded");
pub(crate) fn add(a: Point, b: Point) -> Point {
    Point {
        x: a.x + b.x,
        y: a.y + b.y,
    }
}
pub(crate) fn sub(a: Point, b: Point) -> Point {
    Point {
        x: a.x - b.x,
        y: a.y - b.y,
    }
}
pub(crate) fn mul(a: Point, s: f64) -> Point {
    Point {
        x: a.x * s,
        y: a.y * s,
    }
}
pub(crate) fn length(a: Point) -> f64 {
    a.x.hypot(a.y)
}
pub(crate) fn physical_ok(p: Point) -> bool {
    p.x.is_finite()
        && p.y.is_finite()
        && (-8_388_608. / 40. ..=8_388_607. / 40.).contains(&p.x)
        && (-8_388_608. / 40. ..=8_388_607. / 40.).contains(&p.y)
}
pub(crate) fn map(space: &Space, p: Point, vector: bool) -> Result<Point, TransformError> {
    let p = mm(space.map(p, vector)?);
    if physical_ok(p) {
        Ok(p)
    } else {
        Err(("COORDINATE_RANGE", "Curve coordinate outside profile range"))
    }
}
pub(crate) fn sweep(a: f64) -> f64 {
    let a = a.clamp(-32768., 32767.);
    if a.abs() > 360. {
        a % 360.
    } else {
        a
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn arc(
    space: &Space,
    center: Point,
    radius: f64,
    start: f64,
    sweep: f64,
    chord: ChordTolerance,
    max_segments: usize,
) -> Result<Geometry, TransformError> {
    let value = chord.value;
    if !radius.is_finite() || !start.is_finite() || !sweep.is_finite() || !value.is_finite() {
        return Err(INVALID);
    }
    let step = if chord.mode == 0 {
        let a = value.abs() % 360.;
        (if a > 180. { 360. - a } else { a }).max(0.5)
    } else {
        if value <= 0. {
            return Err(INVALID);
        }
        if radius.abs() <= value {
            180.
        } else {
            // Stable inverse sagitta expression for small deviation/radius ratios.
            (4. * (value / (2. * radius.abs())).sqrt().min(1.).asin()).to_degrees()
        }
    };
    let n = (sweep.abs() / step).ceil().max(1.);
    if !n.is_finite() || n > max_segments as f64 {
        return Err(LIMIT);
    }
    let n = n as usize;
    let center = map(space, center, false)?;
    let u = map(space, Point { x: radius, y: 0. }, true)?;
    let v = map(space, Point { x: 0., y: radius }, true)?;
    let evaluate = |a: f64| {
        add(
            center,
            add(mul(u, a.to_radians().cos()), mul(v, a.to_radians().sin())),
        )
    };
    let mut points = Vec::with_capacity(n + 1);
    for i in 0..=n {
        let p = evaluate(start + sweep * i as f64 / n as f64);
        if !physical_ok(p) {
            return Err(("COORDINATE_RANGE", "Arc exceeds profile range"));
        }
        points.push(p);
    }
    if sweep.abs() == 360. {
        points[n] = points[0];
    }
    Ok(Geometry::Curve {
        curve: Curve::EllipticArc {
            center,
            axis_u: u,
            axis_v: v,
            start_angle_deg: start,
            sweep_angle_deg: sweep,
            chord,
        },
        points,
        start_dot: radius == 0.,
    })
}

pub(crate) fn bezier(control: [Point; 4], max_segments: usize) -> Result<Geometry, TransformError> {
    const TOL: f64 = 0.01;
    if !control.iter().all(|p| physical_ok(*p)) {
        return Err((
            "COORDINATE_RANGE",
            "Bezier control point outside profile range",
        ));
    }
    let mut points = vec![control[0]];
    let mut stack = vec![(control, 0u8)];
    while let Some((c, depth)) = stack.pop() {
        let chord = sub(c[3], c[0]);
        let len = length(chord);
        let distance = |p: Point| {
            if len == 0. {
                length(sub(p, c[0]))
            } else {
                let t = ((p.x - c[0].x) * chord.x + (p.y - c[0].y) * chord.y) / (len * len);
                length(sub(p, add(c[0], mul(chord, t.clamp(0., 1.)))))
            }
        };
        if distance(c[1]).max(distance(c[2])) <= TOL {
            if points.len() > max_segments {
                return Err(LIMIT);
            }
            points.push(c[3]);
        } else {
            if depth >= 24 || points.len() + stack.len() >= max_segments {
                return Err(LIMIT);
            }
            let a = mul(add(c[0], c[1]), 0.5);
            let b = mul(add(c[1], c[2]), 0.5);
            let d = mul(add(c[2], c[3]), 0.5);
            let e = mul(add(a, b), 0.5);
            let f = mul(add(b, d), 0.5);
            let mid = mul(add(e, f), 0.5);
            stack.push(([mid, f, d, c[3]], depth + 1));
            stack.push(([c[0], a, e, mid], depth + 1));
        }
    }
    Ok(Geometry::Curve {
        curve: Curve::CubicBezier {
            control_points: control,
            tolerance_mm: TOL,
        },
        points,
        start_dot: false,
    })
}

pub(crate) fn three_point(
    a: Point,
    b: Point,
    c: Point,
) -> Result<Option<(Point, f64, f64, f64)>, TransformError> {
    if a == b || b == c {
        return Ok(None);
    }
    let u = sub(b, a);
    let v = sub(c, a);
    if a == c {
        let center = mul(add(a, b), 0.5);
        let r = sub(a, center);
        return Ok(Some((center, length(r), r.y.atan2(r.x).to_degrees(), 360.)));
    }
    let cross = u.x * v.y - u.y * v.x;
    if cross.abs() <= 1e-12 * length(u) * length(v) {
        let t = (u.x * v.x + u.y * v.y) / (v.x * v.x + v.y * v.y);
        if (0. ..=1.).contains(&t) {
            return Ok(None);
        }
        return Err((
            "UNSUPPORTED_GEOMETRY",
            "Collinear three-point arc with outside intermediate point requires unbounded rays",
        ));
    }
    let uu = u.x * u.x + u.y * u.y;
    let vv = v.x * v.x + v.y * v.y;
    let center = add(
        a,
        Point {
            x: (uu * v.y - vv * u.y) / (2. * cross),
            y: (u.x * vv - v.x * uu) / (2. * cross),
        },
    );
    let start = (a.y - center.y).atan2(a.x - center.x).to_degrees();
    let end = (c.y - center.y).atan2(c.x - center.x).to_degrees();
    let sweep = if cross > 0. {
        (end - start).rem_euclid(360.)
    } else {
        -((start - end).rem_euclid(360.))
    };
    Ok(Some((center, length(sub(a, center)), start, sweep)))
}
