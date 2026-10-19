use std::fmt::{self, Write};

use crate::{Bounds, ClipRegion, Curve, Geometry, PlotDocument, Point, Style};

#[derive(Debug)]
pub enum ExportError {
    PageIndex(usize),
    InvalidMargin,
    InvalidGeometry,
    RenderLimit,
    Json(serde_json::Error),
}

impl fmt::Display for ExportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::PageIndex(index) => write!(f, "Page index {index} is out of range"),
            Self::InvalidMargin => {
                write!(f, "SVG margin must be finite and between 0 and 1000000 mm")
            }
            Self::InvalidGeometry => write!(f, "Document contains invalid geometry or style"),
            Self::RenderLimit => write!(f, "SVG pattern expansion limit exceeded"),
            Self::Json(e) => e.fmt(f),
        }
    }
}
impl std::error::Error for ExportError {}

impl PlotDocument {
    /// Stable schema v1; optional retained source bytes/commands are not embedded.
    pub fn to_json(&self) -> Result<String, ExportError> {
        self.validate_export()?;
        serde_json::to_string_pretty(self).map_err(ExportError::Json)
    }

    fn validate_export(&self) -> Result<(), ExportError> {
        let finite =
            |p: Point| p.x.is_finite() && p.y.is_finite() && p.x.abs() <= 1e9 && p.y.abs() <= 1e9;
        let valid_clip = |clip: Option<ClipRegion>| match clip {
            Some(ClipRegion::Rect { min, max }) => {
                finite(min) && finite(max) && min.x <= max.x && min.y <= max.y
            }
            _ => true,
        };
        if !finite(self.final_state.position)
            || self
                .final_state
                .text
                .as_ref()
                .is_some_and(|t| !valid_text(t))
            || !self.device_profile.validate()
            || self
                .final_state
                .transform
                .as_ref()
                .is_some_and(|s| s.matrix.iter().any(|v| !v.is_finite()) || !valid_clip(s.clip))
            || self.pages.iter().any(|page| {
                page.bounds.is_some_and(|b| {
                    !finite(b.min) || !finite(b.max) || b.max.x < b.min.x || b.max.y < b.min.y
                }) || page
                    .paper_size_mm
                    .is_some_and(|s| s.iter().any(|v| !v.is_finite() || *v <= 0.0))
                    || page
                        .plot_size_mm
                        .is_some_and(|s| s.iter().any(|v| !v.is_finite() || *v <= 0.0))
                    || page.operations.iter().any(|op| {
                        !valid_clip(op.clip)
                            || !valid_geometry(&op.geometry)
                            || !op.style.width_mm.is_finite()
                            || !(0.0..=1e6).contains(&op.style.width_mm)
                            || op.style.color.len() != 7
                            || !op.style.color.starts_with('#')
                            || !op.style.color.as_bytes()[1..]
                                .iter()
                                .all(u8::is_ascii_hexdigit)
                            || op.style.line.as_ref().is_some_and(|l| {
                                ![1, 2, 4].contains(&l.cap)
                                    || ![1, 2, 4, 5, 6].contains(&l.join)
                                    || !l.miter_limit.is_finite()
                                    || !(1. ..=32767.).contains(&l.miter_limit)
                                    || !l.phase_mm.is_finite()
                                    || l.phase_mm < 0.
                                    || l.line_type.is_some_and(|k| !(-8..=8).contains(&k))
                                    || l.pattern_length_mm.is_some_and(|n| {
                                        !n.is_finite() || !(1e-6..=1e6).contains(&n)
                                    })
                                    || (l.line_type.is_some_and(|k| k != 0)
                                        && l.pattern_length_mm.is_none())
                            })
                    })
            })
        {
            return Err(ExportError::InvalidGeometry);
        }
        Ok(())
    }

    /// Cropped preview in mm, Y up. Paper size is never inferred from drawing bounds.
    pub fn to_svg(&self, page_index: usize, margin_mm: f64) -> Result<String, ExportError> {
        if !margin_mm.is_finite() || !(0.0..=1e6).contains(&margin_mm) {
            return Err(ExportError::InvalidMargin);
        }
        let page = self
            .pages
            .get(page_index)
            .ok_or(ExportError::PageIndex(page_index))?;
        self.validate_export()?;
        // Includes stroke expansion, then intersects the operation's clip window.
        let margin = margin_mm;
        let b = page.paint_bounds().unwrap_or(Bounds {
            min: Point::default(),
            max: Point { x: 1.0, y: 1.0 },
        });
        let width = (b.max.x - b.min.x + 2.0 * margin).max(1.0);
        let height = (b.max.y - b.min.y + 2.0 * margin).max(1.0);
        let x = (b.min.x + b.max.x - width) / 2.0;
        let y = (-b.min.y - b.max.y - height) / 2.0;
        let completeness = if self.completeness == "complete" {
            "complete"
        } else {
            "partial"
        };
        let mut svg = format!("<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{width}mm\" height=\"{height}mm\" viewBox=\"{x} {y} {width} {height}\" data-completeness=\"{completeness}\" data-omitted-content=\"{}\">\n<title>HP-GL page {} ({completeness})</title>\n<rect x=\"{x}\" y=\"{y}\" width=\"{width}\" height=\"{height}\" fill=\"#ffffff\"/>\n<g transform=\"scale(1,-1)\" fill=\"none\" stroke-linecap=\"butt\" stroke-linejoin=\"miter\" stroke-miterlimit=\"5\">\n", page.omitted_content, page_index + 1);
        let mut index = 0;
        let mut budget = 2_000_000usize;
        while index < page.operations.len() {
            let op = &page.operations[index];
            let start = index;
            index += 1;
            while index < page.operations.len()
                && !matches!(
                    op.geometry,
                    Geometry::Polygon { .. } | Geometry::Label { .. }
                )
                && !matches!(
                    page.operations[index].geometry,
                    Geometry::Polygon { .. } | Geometry::Label { .. }
                )
                && page.operations[index].path_id == op.path_id
                && same_style(&page.operations[index].style, &op.style)
                && page.operations[index].clip == op.clip
                && page.operations[index - 1].geometry.points().last()
                    == page.operations[index].geometry.points().first()
            {
                index += 1;
            }
            if matches!(op.clip, Some(ClipRegion::Empty)) {
                continue;
            }
            if let Some(ClipRegion::Rect { min, max }) = op.clip {
                writeln!(svg, "<defs><clipPath id=\"clip-{start}\" clipPathUnits=\"userSpaceOnUse\"><rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\"/></clipPath></defs><g clip-path=\"url(#clip-{start})\">", min.x, min.y, max.x-min.x, max.y-min.y).unwrap();
            }
            if let Geometry::Label { label } = &op.geometry {
                for g in label.glyphs.iter().filter(|g| !g.character.is_whitespace()) {
                    consume(&mut budget, 4)?;
                    let ch = match g.character {
                        '&' => "&amp;".into(),
                        '<' => "&lt;".into(),
                        '>' => "&gt;".into(),
                        '"' => "&quot;".into(),
                        '\'' => "&apos;".into(),
                        c => c.to_string(),
                    };
                    // SVG font coordinates are Y down; the outer drawing group is Y up.
                    writeln!(svg, "<text data-approximate=\"true\" data-pen=\"{}\" font-family=\"monospace\" font-size=\"1.4\" textLength=\"1\" lengthAdjust=\"spacingAndGlyphs\" fill=\"{}\" stroke=\"none\" transform=\"matrix({} {} {} {} {} {})\">{ch}</text>", op.style.pen, op.style.color, g.axis_x.x, g.axis_x.y, -g.axis_y.x, -g.axis_y.y, g.origin.x, g.origin.y).unwrap();
                }
            } else if let Geometry::Polygon { contours, fill } = &op.geometry {
                if let Some(fill) = fill {
                    if fill.fill_type < 3 {
                        write!(svg,"<path data-pen=\"{}\" fill=\"{}\" stroke=\"none\" fill-rule=\"{}\" d=\"",op.style.pen,op.style.color,fill.rule).unwrap();
                        for c in contours.iter().filter(|c| c.points.len() >= 3) {
                            path_data(&mut svg, &c.points);
                            svg.push_str(" Z ");
                        }
                        svg.push_str("\"/>\n");
                        for c in contours
                            .iter()
                            .filter(|c| c.points.len() >= 3 && collinear(&c.points))
                        {
                            stroke(&mut svg, &c.points, &op.style, &mut budget)?;
                            if c.points.iter().all(|p| *p == c.points[0]) {
                                dot(&mut svg, c.points[0], &op.style);
                            }
                        }
                    } else {
                        for (index, line) in fill.hatch_lines.iter().enumerate() {
                            let mut style = op.style.clone();
                            if let Some(l) = &mut style.line {
                                l.phase_mm = fill.hatch_phases_mm.get(index).copied().unwrap_or(0.);
                            }
                            stroke(&mut svg, line, &style, &mut budget)?;
                        }
                    }
                } else {
                    for c in contours.iter().filter(|c| c.points.len() >= 3) {
                        let mut run = Vec::new();
                        for e in &c.edges {
                            if e.pen_down {
                                if run.is_empty() {
                                    run.extend_from_slice(&c.points[e.start..=e.end]);
                                } else {
                                    run.extend_from_slice(&c.points[e.start + 1..=e.end]);
                                }
                            } else if !run.is_empty() {
                                stroke(&mut svg, &run, &op.style, &mut budget)?;
                                run.clear();
                            }
                        }
                        if !run.is_empty() {
                            stroke(&mut svg, &run, &op.style, &mut budget)?;
                            if run.len() >= 3 && run.iter().all(|p| *p == run[0]) {
                                dot(&mut svg, run[0], &op.style);
                            }
                        }
                    }
                }
            } else {
                let run = &page.operations[start..index];
                let mut points = op.geometry.points().to_vec();
                for segment in &run[1..] {
                    points.extend_from_slice(&segment.geometry.points()[1..]);
                }
                stroke(&mut svg, &points, &op.style, &mut budget)?;
                for segment in run {
                    if segment.geometry.start_dot()
                        && op
                            .style
                            .line
                            .as_ref()
                            .is_none_or(|l| l.line_type != Some(0))
                    {
                        dot(&mut svg, segment.geometry.points()[0], &segment.style);
                    }
                }
            }
            if op.clip.is_some() {
                svg.push_str("</g>\n");
            }
        }
        svg.push_str("</g>\n</svg>\n");
        Ok(svg)
    }
}

fn valid_point(p: Point) -> bool {
    p.x.is_finite() && p.y.is_finite() && p.x.abs() <= 1e9 && p.y.abs() <= 1e9
}
fn valid_curve(c: &Curve) -> bool {
    match c {
        Curve::EllipticArc {
            center,
            axis_u,
            axis_v,
            start_angle_deg,
            sweep_angle_deg,
            chord,
        } => {
            [*center, *axis_u, *axis_v].into_iter().all(valid_point)
                && start_angle_deg.is_finite()
                && sweep_angle_deg.is_finite()
                && sweep_angle_deg.abs() <= 360.
                && chord.mode <= 1
                && chord.value.is_finite()
        }
        Curve::CubicBezier {
            control_points,
            tolerance_mm,
        } => {
            control_points.iter().all(|p| valid_point(*p))
                && tolerance_mm.is_finite()
                && *tolerance_mm > 0.
        }
    }
}
fn valid_font(f: &crate::FontSpec) -> bool {
    f.pitch.is_finite() && f.pitch > 0. && f.height_points.is_finite() && f.height_points >= 0.
}
fn valid_text(t: &crate::TextState) -> bool {
    valid_font(&t.standard)
        && valid_font(&t.alternate)
        && [t.width, t.height, t.slant].iter().all(|v| v.is_finite())
        && [t.direction, t.extra_space, t.carriage_return]
            .into_iter()
            .all(valid_point)
}
fn valid_geometry(g: &Geometry) -> bool {
    if g.all_points().any(|p| !valid_point(*p)) {
        return false;
    }
    match g {
        Geometry::Label { label } => {
            valid_text(&label.state)
                && [label.start, label.end].into_iter().all(valid_point)
                && label.glyphs.iter().all(|g| {
                    valid_font(&g.font)
                        && [g.origin, g.axis_x, g.axis_y].into_iter().all(valid_point)
                        && g.envelope.iter().copied().all(valid_point)
                        && !g.character.is_control()
                        && g.character != '\u{fffe}'
                        && g.character != '\u{ffff}'
                })
        }
        Geometry::Polyline { points, .. } => !points.is_empty(),
        Geometry::Curve { points, curve, .. } => !points.is_empty() && valid_curve(curve),
        Geometry::Polygon { contours, fill } => {
            !contours.is_empty()
                && contours.iter().all(|c| {
                    !c.points.is_empty()
                        && c.edges.iter().all(|e| {
                            e.start <= e.end
                                && e.end < c.points.len()
                                && e.curve.as_ref().is_none_or(valid_curve)
                        })
                })
                && fill.as_ref().is_none_or(|f| {
                    [1, 2, 3, 4].contains(&f.fill_type)
                        && ["evenodd", "nonzero"].contains(&f.rule)
                        && f.angle_deg.is_finite()
                        && f.spacing_mm.is_none_or(|v| v.is_finite() && v > 0.)
                        && f.hatch_lines.iter().flatten().all(|p| valid_point(*p))
                        && (f.hatch_phases_mm.is_empty()
                            || f.hatch_phases_mm.len() == f.hatch_lines.len())
                        && f.hatch_phases_mm.iter().all(|v| v.is_finite() && *v >= 0.)
                })
        }
    }
}
fn same_style(a: &Style, b: &Style) -> bool {
    let mut a = a.clone();
    let mut b = b.clone();
    if let Some(l) = &mut a.line {
        l.phase_mm = 0.;
    }
    if let Some(l) = &mut b.line {
        l.phase_mm = 0.;
    }
    a == b
}
fn path_data(svg: &mut String, points: &[Point]) {
    if let Some(p) = points.first() {
        write!(svg, "M{} {}", p.x, p.y).unwrap();
    }
    for p in &points[1..] {
        write!(svg, " L{} {}", p.x, p.y).unwrap();
    }
}
fn dot(svg: &mut String, p: Point, s: &Style) {
    writeln!(
        svg,
        "<circle data-pen=\"{}\" cx=\"{}\" cy=\"{}\" r=\"{}\" fill=\"{}\"/>",
        s.pen,
        p.x,
        p.y,
        s.width_mm / 2.,
        s.color
    )
    .unwrap();
}
fn path(svg: &mut String, points: &[Point], s: &Style, dash: &[f64], phase: f64) {
    write!(
        svg,
        "<path data-pen=\"{}\" stroke=\"{}\" stroke-width=\"{}\"",
        s.pen, s.color, s.width_mm
    )
    .unwrap();
    if let Some(l) = &s.line {
        let cap = match l.cap {
            2 => "square",
            4 => "round",
            _ => "butt",
        };
        let join = match l.join {
            4 => "round",
            5 => "bevel",
            _ => "miter",
        };
        write!(
            svg,
            " stroke-linecap=\"{cap}\" stroke-linejoin=\"{join}\" stroke-miterlimit=\"{}\"",
            l.miter_limit
        )
        .unwrap();
    }
    if !dash.is_empty() {
        svg.push_str(" stroke-dasharray=\"");
        for v in dash {
            write!(svg, "{v} ").unwrap();
        }
        write!(svg, "\" stroke-dashoffset=\"{}\"", -phase).unwrap();
    }
    svg.push_str(" d=\"");
    path_data(svg, points);
    if points.len() > 2 && points.first() == points.last() {
        svg.push_str(" Z");
    }
    svg.push_str("\"/>\n");
}
fn mark(svg: &mut String, p: Point, direction: Point, s: &Style) {
    let length = direction.x.hypot(direction.y);
    let v = if length == 0. {
        Point { x: 0.025, y: 0. }
    } else {
        Point {
            x: direction.x / length * 0.025,
            y: direction.y / length * 0.025,
        }
    };
    path(
        svg,
        &[
            p,
            Point {
                x: p.x + v.x,
                y: p.y + v.y,
            },
        ],
        s,
        &[],
        0.,
    );
}
fn consume(budget: &mut usize, n: usize) -> Result<(), ExportError> {
    *budget = budget.checked_sub(n).ok_or(ExportError::RenderLimit)?;
    Ok(())
}
fn stroke(
    svg: &mut String,
    points: &[Point],
    s: &Style,
    budget: &mut usize,
) -> Result<(), ExportError> {
    if points.is_empty() {
        return Ok(());
    }
    consume(budget, points.len())?;
    let kind = s.line.as_ref().and_then(|l| l.line_type);
    if kind == Some(0) {
        if points.len() == 1 {
            mark(svg, points[0], Point::default(), s);
        }
        for pair in points.windows(2) {
            mark(svg, pair[1], crate::curves::sub(pair[1], pair[0]), s);
        }
        return Ok(());
    }
    let split = s
        .line
        .as_ref()
        .is_some_and(|l| l.join == 6 || l.line_type.is_some_and(|k| k < 0));
    if split && points.len() > 2 {
        let mut s = s.clone();
        for pair in points.windows(2) {
            stroke_part(svg, pair, &s, budget)?;
            if let Some(l) = &mut s.line {
                if let Some(n) = l.pattern_length_mm {
                    l.phase_mm = (l.phase_mm
                        + crate::curves::length(crate::curves::sub(pair[1], pair[0])))
                        % n;
                }
            }
        }
        Ok(())
    } else {
        stroke_part(svg, points, s, budget)
    }
}
fn stroke_part(
    svg: &mut String,
    points: &[Point],
    s: &Style,
    budget: &mut usize,
) -> Result<(), ExportError> {
    let Some(line) = s
        .line
        .as_ref()
        .filter(|l| l.line_type.is_some_and(|k| k != 0))
    else {
        path(svg, points, s, &[], 0.);
        return Ok(());
    };
    let kind = line.line_type.unwrap();
    let nominal = line.pattern_length_mm.unwrap();
    let total: f64 = points
        .windows(2)
        .map(|p| crate::curves::length(crate::curves::sub(p[1], p[0])))
        .sum();
    let period = if kind < 0 && total > 0. {
        total / (total / nominal).ceil().max(1.)
    } else {
        nominal
    };
    let mut percents = crate::attributes::pattern(kind).to_vec();
    if kind < 0 {
        let half = percents[0] / 2.;
        percents[0] = half;
        percents.extend([half, 0.]);
    }
    let repeats = (total / period).ceil() + 2.;
    if !repeats.is_finite() || repeats * percents.len() as f64 > *budget as f64 {
        return Err(ExportError::RenderLimit);
    }
    consume(budget, repeats as usize * percents.len())?;
    let dash: Vec<f64> = percents.iter().map(|v| v * period / 100.).collect();
    let phase = if kind < 0 { 0. } else { line.phase_mm };
    path(svg, points, s, &dash, phase);
    let mut offset = 0.;
    let mut dot_offsets = Vec::new();
    for (i, v) in dash.iter().enumerate() {
        if i % 2 == 0 && *v == 0. {
            dot_offsets.push(offset);
        }
        offset += v;
    }
    let mut travelled = 0.;
    for pair in points.windows(2) {
        let d = crate::curves::sub(pair[1], pair[0]);
        let length = crate::curves::length(d);
        if length == 0. {
            continue;
        }
        for dot_offset in &dot_offsets {
            let mut at =
                ((travelled + phase - dot_offset) / period).ceil() * period + dot_offset - phase;
            while at <= travelled + length + 1e-10 {
                let t = ((at - travelled) / length).clamp(0., 1.);
                mark(
                    svg,
                    Point {
                        x: pair[0].x + t * d.x,
                        y: pair[0].y + t * d.y,
                    },
                    d,
                    s,
                );
                at += period;
            }
        }
        travelled += length;
    }
    Ok(())
}
fn collinear(points: &[Point]) -> bool {
    let a = points[0];
    let Some(b) = points.iter().find(|p| **p != a) else {
        return true;
    };
    let d = crate::curves::sub(*b, a);
    points
        .iter()
        .all(|p| ((p.x - a.x) * d.y - (p.y - a.y) * d.x).abs() < 1e-10)
}
