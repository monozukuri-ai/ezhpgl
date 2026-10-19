use super::*;
use crate::curves::{add, length, sub};

impl Interpreter {
    fn provenance(c: &Command, index: usize) -> Provenance {
        Provenance {
            command_index: index,
            command: c.name().into(),
            span: c.span,
        }
    }
    #[allow(clippy::too_many_arguments)] // Geometry, pen state, and command provenance.
    pub(super) fn buffer_path(
        &self,
        p: &mut Pending,
        points: Vec<Point>,
        curve: Option<Curve>,
        down: bool,
        skip_start: bool,
        c: &Command,
        index: usize,
    ) -> Result<(), ParseError> {
        let limits = self.options.geometry_limits;
        p.state.polygon.approximate |= p.state.position_approximate;
        let append = p
            .state
            .polygon
            .append(
                points,
                curve,
                down,
                skip_start,
                Self::provenance(c, index),
                (limits.max_polygon_vertices, limits.max_polygon_contours),
            )
            .map_err(|(code, message)| error(code, message, c.span, Some(c.name())))?;
        p.buffered.push(append);
        Ok(())
    }
    fn close_polygon(&self, p: &mut Pending, c: &Command, index: usize) -> Result<(), ParseError> {
        if let (Some(first), Some(last)) = (p.state.polygon.first, p.state.polygon.last) {
            if first != last {
                self.buffer_path(p, vec![last, first], None, false, false, c, index)?;
            }
            p.state.position = Point {
                x: first.x * 40.,
                y: first.y * 40.,
            };
            p.state.position_approximate |= p.state.polygon.approximate;
        }
        p.state.polygon.first = None;
        p.state.polygon.last = None;
        p.state.polygon.current_points = 0;
        Ok(())
    }
    pub(super) fn advance_phase(p: &mut Pending, a: Point, b: Point) -> Result<(), ParseError> {
        p.state.attributes.last_end = Some(b);
        if p.state.attributes.line.kind.is_some_and(|k| k > 0) {
            let n = p
                .state
                .attributes
                .pattern_length(&p.state.space)
                .map_err(|(code, message)| error(code, message, SourceSpan::default(), None))?
                .unwrap();
            p.state.attributes.phase = (p.state.attributes.phase + length(sub(b, a))) % n;
        }
        Ok(())
    }
    pub(super) fn operation(
        &self,
        p: &mut Pending,
        geometry: Geometry,
        c: &Command,
        index: usize,
    ) -> Result<(), ParseError> {
        if p.state.pen == 0 {
            return Ok(());
        }
        let n = geometry.vertex_count();
        let limits = self.options.geometry_limits;
        if self.pages.len() >= limits.max_pages
            || p.operations.len() >= limits.max_operations.saturating_sub(self.operations)
            || n > limits
                .max_vertices
                .saturating_sub(self.vertices)
                .saturating_sub(p.vertices)
        {
            return Err(error(
                "LIMIT_EXCEEDED",
                "Page, operation or vertex limit exceeded",
                c.span,
                Some(c.name()),
            ));
        }
        if let Some(&start) = geometry.points().first() {
            p.state.attributes.begin_stroke(start);
        } else {
            p.state.attributes.phase = 0.;
        }
        let style = p
            .state
            .attributes
            .snapshot(p.state.pen, &p.state.space)
            .map_err(|(code, message)| error(code, message, c.span, Some(c.name())))?;
        let clip = p
            .state
            .space
            .clip()
            .map_err(|(code, message)| error(code, message, c.span, Some(c.name())))?;
        for pair in geometry.points().windows(2) {
            Self::advance_phase(p, pair[0], pair[1])?;
        }
        p.operations.push(PlotOperation {
            approximate: p.state.position_approximate || matches!(geometry, Geometry::Label { .. }),
            geometry,
            style,
            source: Self::provenance(c, index),
            path_id: p.state.path_id,
            clip,
        });
        p.vertices += n;
        Ok(())
    }
    fn path(
        &self,
        p: &mut Pending,
        geometry: Geometry,
        force: bool,
        c: &Command,
        index: usize,
    ) -> Result<(), ParseError> {
        if p.state.polygon.active {
            let curve = geometry.curve().cloned();
            self.buffer_path(
                p,
                geometry.points().to_vec(),
                curve,
                p.state.down || force,
                false,
                c,
                index,
            )
        } else if p.state.down || force {
            self.operation(p, geometry, c, index)
        } else {
            Ok(())
        }
    }
    fn chord(&self, a: Option<f64>) -> ChordTolerance {
        ChordTolerance {
            mode: self.state.chord_mode,
            value: a.unwrap_or(5.),
        }
    }
    #[allow(clippy::too_many_arguments)] // Arc parameters plus diagnostic command context.
    fn arc(
        &self,
        space: &Space,
        center: Point,
        radius: f64,
        start: f64,
        sweep: f64,
        chord: ChordTolerance,
        c: &Command,
    ) -> Result<Geometry, ParseError> {
        curves::arc(
            space,
            center,
            radius,
            start,
            sweep,
            chord,
            self.options.geometry_limits.max_curve_segments,
        )
        .map_err(|(code, message)| error(code, message, c.span, Some(c.name())))
    }
    pub(super) fn drawing(
        &mut self,
        c: &Command,
        index: usize,
        a: &[f64],
    ) -> Result<(), ParseError> {
        let fail = |code, message| error(code, message, c.span, Some(c.name()));
        let invalid = || fail("INVALID_ARGUMENTS", "Invalid drawing command arguments");
        if a.iter().any(|v| !v.is_finite() || v.abs() > 1_073_741_824.) {
            return Err(invalid());
        }
        let transform = |(code, message)| fail(code, message);
        match c.name() {
            "CT" => {
                if a.len() > 1 {
                    return Err(invalid());
                }
                let mode = a.first().copied().unwrap_or(0.).round();
                if mode != 0. && mode != 1. {
                    return Err(invalid());
                }
                self.state.chord_mode = mode as u8;
            }
            "PM" => {
                if a.len() > 1 {
                    return Err(invalid());
                }
                let mode = a.first().copied().unwrap_or(0.).round();
                if ![0., 1., 2.].contains(&mode) {
                    return Err(invalid());
                }
                let mut p = self.pending();
                if mode == 0. {
                    p.state.polygon = polygon::Cursor {
                        active: true,
                        ..Default::default()
                    };
                    let position = mm(p.state.position);
                    self.buffer_path(&mut p, vec![position], None, false, false, c, index)?;
                    self.buffer = Arc::default();
                    self.buffer_approximate = false;
                } else if p.state.polygon.active {
                    self.close_polygon(&mut p, c, index)?;
                    p.state.polygon.active = mode == 1.;
                }
                p.state.path_id += 1;
                self.commit(p);
            }
            "FP" | "EP" => {
                if (c.name() == "EP" && !a.is_empty()) || a.len() > 1 {
                    return Err(invalid());
                }
                let rule = a.first().copied().unwrap_or(0.).round();
                if rule != 0. && rule != 1. {
                    return Err(invalid());
                }
                if self.state.polygon.active || self.buffer.is_empty() {
                    return Ok(());
                }
                let mut p = self.pending();
                p.state.path_id += 1;
                self.paint_buffer(
                    &mut p,
                    self.buffer.clone(),
                    c.name() == "FP",
                    rule as u8,
                    c,
                    index,
                )?;
                p.state.attributes = self.state.attributes.clone();
                p.state.path_id += 1;
                self.commit(p);
            }
            "CI" => {
                if !matches!(a.len(), 1 | 2) {
                    return Err(invalid());
                }
                let mut p = self.pending();
                if p.state.polygon.active {
                    self.close_polygon(&mut p, c, index)?;
                }
                let center = p.state.space.unmap(p.state.position).map_err(transform)?;
                let g = self.arc(
                    &p.state.space,
                    center,
                    a[0].abs(),
                    if a[0] < 0. { 180. } else { 0. },
                    360.,
                    self.chord(a.get(1).copied()),
                    c,
                )?;
                p.state.path_id += 1;
                let at = p.state.position;
                self.path(&mut p, g, true, c, index)?;
                if p.state.polygon.active {
                    self.close_polygon(&mut p, c, index)?;
                }
                p.state.position = at;
                p.state.attributes = self.state.attributes.clone();
                p.state.path_id += 1;
                self.commit(p);
            }
            "AA" | "AR" | "AT" | "RT" => {
                let three = matches!(c.name(), "AT" | "RT");
                let base = if three { 4 } else { 3 };
                if a.len() != base && a.len() != base + 1 {
                    return Err(invalid());
                }
                let mut p = self.pending();
                let start = p.state.space.unmap(p.state.position).map_err(transform)?;
                let relative = matches!(c.name(), "AR" | "RT");
                let coord = |x: f64, y: f64| {
                    if relative {
                        add(start, Point { x, y })
                    } else {
                        Point { x, y }
                    }
                };
                let chord = self.chord(a.get(base).copied());
                let mut g = if three {
                    let mid = coord(a[0], a[1]);
                    let end = coord(a[2], a[3]);
                    match curves::three_point(start, mid, end).map_err(transform)? {
                        Some((center, radius, angle, sweep)) => {
                            let mut g =
                                self.arc(&p.state.space, center, radius, angle, sweep, chord, c)?;
                            if let Geometry::Curve { points, .. } = &mut g {
                                let n = points.len();
                                points[n - 1] =
                                    curves::map(&p.state.space, end, false).map_err(transform)?;
                            }
                            g
                        }
                        None => Geometry::Polyline {
                            points: vec![
                                mm(p.state.position),
                                curves::map(&p.state.space, end, false).map_err(transform)?,
                            ],
                            start_dot: start == end,
                        },
                    }
                } else {
                    let center = coord(a[0], a[1]);
                    let v = sub(start, center);
                    self.arc(
                        &p.state.space,
                        center,
                        length(v),
                        v.y.atan2(v.x).to_degrees(),
                        curves::sweep(a[2]),
                        chord,
                        c,
                    )?
                };
                // Preserve exact continuity at the original cursor.
                match &mut g {
                    Geometry::Curve { points, .. } | Geometry::Polyline { points, .. } => {
                        points[0] = mm(p.state.position)
                    }
                    _ => unreachable!(),
                }
                let end = *g.points().last().unwrap();
                self.path(&mut p, g, false, c, index)?;
                p.state.position = Point {
                    x: end.x * 40.,
                    y: end.y * 40.,
                };
                self.commit(p);
            }
            "BZ" | "BR" => {
                if a.is_empty() || a.len() % 6 != 0 {
                    return Err(invalid());
                }
                let mut p = self.pending();
                for args in a.chunks_exact(6) {
                    let start = p.state.space.unmap(p.state.position).map_err(transform)?;
                    let mut controls = [mm(p.state.position); 4];
                    for (i, xy) in args.chunks_exact(2).enumerate() {
                        let point = Point { x: xy[0], y: xy[1] };
                        controls[i + 1] = curves::map(
                            &p.state.space,
                            if c.name() == "BR" {
                                add(start, point)
                            } else {
                                point
                            },
                            false,
                        )
                        .map_err(transform)?;
                    }
                    let g =
                        curves::bezier(controls, self.options.geometry_limits.max_curve_segments)
                            .map_err(transform)?;
                    self.path(&mut p, g, false, c, index)?;
                    p.state.position = Point {
                        x: controls[3].x * 40.,
                        y: controls[3].y * 40.,
                    };
                }
                self.commit(p);
            }
            "EA" | "ER" | "RA" | "RR" | "EW" | "WG" => {
                let wedge = matches!(c.name(), "EW" | "WG");
                if (!wedge && a.len() != 2) || (wedge && !matches!(a.len(), 3 | 4)) {
                    return Err(invalid());
                }
                let mut p = self.pending();
                let original = p.state.clone();
                p.state.polygon = polygon::Cursor {
                    active: true,
                    ..Default::default()
                };
                let start = p.state.space.unmap(p.state.position).map_err(transform)?;
                if wedge {
                    let sweep = a[2].clamp(-360., 360.);
                    let g = self.arc(
                        &p.state.space,
                        start,
                        a[0].abs(),
                        a[1].clamp(-32768., 32767.) % 360. + if a[0] < 0. { 180. } else { 0. },
                        sweep,
                        self.chord(a.get(3).copied()),
                        c,
                    )?;
                    if sweep.abs() != 360. {
                        let center = mm(p.state.position);
                        let first = g.points()[0];
                        self.buffer_path(&mut p, vec![center, first], None, true, false, c, index)?;
                    }
                    self.path(&mut p, g, true, c, index)?;
                    // Wedge/rectangle closure is an explicit pen-down edge.
                    let first = p.state.polygon.first.unwrap();
                    let last = p.state.polygon.last.unwrap();
                    self.buffer_path(&mut p, vec![last, first], None, true, false, c, index)?;
                } else {
                    let end = if matches!(c.name(), "ER" | "RR") {
                        add(start, Point { x: a[0], y: a[1] })
                    } else {
                        Point { x: a[0], y: a[1] }
                    };
                    let points = [
                        start,
                        Point {
                            x: end.x,
                            y: start.y,
                        },
                        end,
                        Point {
                            x: start.x,
                            y: end.y,
                        },
                        start,
                    ]
                    .into_iter()
                    .map(|q| curves::map(&p.state.space, q, false))
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(transform)?;
                    self.buffer_path(&mut p, points, None, true, false, c, index)?;
                }
                let mut buffer = Vec::new();
                for append in p.buffered.drain(..) {
                    append.apply(&mut buffer);
                }
                let buffer = Arc::new(buffer);
                let approximate = p.state.polygon.approximate;
                p.state = original;
                p.state.path_id += 1;
                self.paint_buffer(
                    &mut p,
                    buffer.clone(),
                    matches!(c.name(), "RA" | "RR" | "WG"),
                    0,
                    c,
                    index,
                )?;
                p.state.attributes = self.state.attributes.clone();
                p.state.path_id += 1;
                p.state.polygon = polygon::Cursor::default();
                self.buffer = buffer;
                self.buffer_approximate = approximate;
                self.commit(p);
            }
            _ => unreachable!(),
        }
        Ok(())
    }
    fn paint_buffer(
        &self,
        p: &mut Pending,
        contours: Arc<Vec<Contour>>,
        filled: bool,
        rule: u8,
        c: &Command,
        index: usize,
    ) -> Result<(), ParseError> {
        if !contours.iter().any(|v| v.points.len() >= 3) {
            return Ok(());
        }
        let fill = if filled {
            Some(
                polygon::fill(
                    &contours,
                    &p.state.attributes,
                    &p.state.space,
                    rule,
                    self.options.geometry_limits.max_hatch_lines,
                )
                .map_err(|(code, message)| error(code, message, c.span, Some(c.name())))?,
            )
        } else {
            None
        };
        let approximate = p.state.position_approximate;
        if matches!(c.name(), "FP" | "EP") {
            p.state.position_approximate = self.buffer_approximate;
        }
        let result = self.operation(p, Geometry::Polygon { contours, fill }, c, index);
        p.state.position_approximate = approximate;
        result
    }
}
