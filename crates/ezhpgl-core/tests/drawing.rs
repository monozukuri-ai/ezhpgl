use ezhpgl_core::{parse, Curve, Geometry, GeometryLimits, InterpretOptions, PlotDocument, Point};

fn read(s: &str) -> PlotDocument {
    parse(s.as_bytes(), InterpretOptions::default()).unwrap()
}
fn point(x: f64, y: f64) -> Point {
    Point { x, y }
}
fn near(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-8, "{a} != {b}");
}
fn near_point(a: Point, b: Point) {
    near(a.x, b.x);
    near(a.y, b.y);
}
fn curve(d: &PlotDocument, i: usize) -> &Curve {
    d.pages[0].operations[i].geometry.curve().unwrap()
}
fn polygon(
    d: &PlotDocument,
    i: usize,
) -> (&[ezhpgl_core::Contour], Option<&ezhpgl_core::FillStyle>) {
    let Geometry::Polygon { contours, fill } = &d.pages[0].operations[i].geometry else {
        panic!("polygon expected")
    };
    (contours, fill.as_ref())
}
fn partial(s: &str) -> PlotDocument {
    let mut options = InterpretOptions::default();
    options.parse.strict = false;
    parse(s.as_bytes(), options).unwrap()
}

#[test]
fn circle_preserves_cursor_pen_and_analytic_ellipse_under_mirror_scaling() {
    let d = read("IP0,0,800,400;SC10,0,0,10;SP1;PA5,5;CI2,90;PR1,0;");
    assert_eq!(d.stats().operations, 1);
    assert!(!d.final_state.pen_down);
    near_point(d.final_state.position, point(8., 5.));
    let Curve::EllipticArc {
        center,
        axis_u,
        axis_v,
        start_angle_deg,
        sweep_angle_deg,
        chord,
    } = curve(&d, 0)
    else {
        panic!()
    };
    assert_eq!(*center, point(10., 5.));
    assert_eq!(*axis_u, point(-4., 0.));
    assert_eq!(*axis_v, point(0., 2.));
    assert_eq!(
        (*start_angle_deg, *sweep_angle_deg, chord.mode, chord.value),
        (0., 360., 0, 90.)
    );
    let p = d.pages[0].operations[0].geometry.points();
    assert_eq!(p.len(), 5);
    assert_eq!(p.first(), p.last());
    near_point(p[1], point(10., 7.));
}

#[test]
fn negative_and_zero_circles_are_isolated_and_zero_renders_dot() {
    let d = read("SP1;PA80,120;PD;CI-40,90;CI0;PR40,0;");
    assert_eq!(d.final_state.position, point(3., 3.));
    assert!(d.final_state.pen_down);
    near_point(d.pages[0].operations[1].geometry.points()[0], point(1., 3.));
    assert!(d.pages[0].operations[2].geometry.start_dot());
    assert!(d.to_svg(0, 0.).unwrap().contains("cx=\"2\" cy=\"3\""));
}

#[test]
fn absolute_relative_arcs_and_modulo_sweeps_keep_endpoints() {
    for (cmd, expected) in [
        ("AA0,0,90;", point(0., 1.)),
        ("AR-40,0,-90;", point(0., -1.)),
        ("AA0,0,450;", point(0., 1.)),
        ("AA0,0,720;", point(1., 0.)),
    ] {
        let d = read(&format!("SP1;PA40,0;PD;{cmd}"));
        near_point(d.final_state.position, expected);
        near_point(d.pages[0].operations[1].geometry.points()[0], point(1., 0.));
    }
    let d = read("PA40,0;AA0,0,90;");
    assert_eq!(d.stats().operations, 0);
    near_point(d.final_state.position, point(0., 1.));
}

#[test]
fn three_point_arcs_preserve_direction_and_relative_endpoints() {
    for cmd in ["AT0,40,-40,0;", "RT-40,40,-80,0;"] {
        let d = read(&format!("SP1;PA40,0;PD;{cmd}"));
        let Curve::EllipticArc {
            center,
            sweep_angle_deg,
            ..
        } = curve(&d, 1)
        else {
            panic!()
        };
        near_point(*center, point(0., 0.));
        near(*sweep_angle_deg, 180.);
        assert_eq!(d.final_state.position, point(-1., 0.));
    }
    let d = read("SP1;PA40,0;PD;AT0,-40,-40,0;");
    let Curve::EllipticArc {
        sweep_angle_deg, ..
    } = curve(&d, 1)
    else {
        panic!()
    };
    near(*sweep_angle_deg, -180.);
}

#[test]
fn three_point_degeneracies_are_explicit_and_atomic() {
    let d = read("SP1;PD;AT40,0,80,0;");
    assert!(matches!(
        d.pages[0].operations[1].geometry,
        Geometry::Polyline { .. }
    ));
    let d = read("SP1;PD;AT40,0,0,0;");
    let Curve::EllipticArc {
        center,
        sweep_angle_deg,
        ..
    } = curve(&d, 1)
    else {
        panic!()
    };
    assert_eq!(*center, point(0.5, 0.));
    assert_eq!(*sweep_angle_deg, 360.);
    let d = partial("SP1;PD40,0;AT120,0,80,0;PD400,0;");
    assert_eq!(d.diagnostics[0].code, "UNSUPPORTED_GEOMETRY");
    assert_eq!(d.final_state.position, point(1., 0.));
    assert_eq!(d.stats().operations, 1);
}

#[test]
fn cubic_controls_relative_chains_and_collinear_reversal() {
    let d = read("SP1;PD;BR0,40,40,40,40,0,0,-40,40,-40,40,0;");
    let Curve::CubicBezier {
        control_points,
        tolerance_mm,
    } = curve(&d, 2)
    else {
        panic!()
    };
    assert_eq!(
        *control_points,
        [point(1., 0.), point(1., -1.), point(2., -1.), point(2., 0.)]
    );
    assert_eq!(*tolerance_mm, 0.01);
    assert_eq!(d.final_state.position, point(2., 0.));
    let d = read("SP1;PD;BZ400,0,-400,0,0,0;");
    let p = d.pages[0].operations[1].geometry.points();
    assert!(p.iter().any(|p| p.x > 2.));
    assert!(p.iter().any(|p| p.x < -2.));
}

#[test]
fn cubic_chains_do_not_commit_before_an_invalid_tail() {
    let d = partial("SP1;PD40,0;BZ40,40,80,40,80,0,80,40,80,8388608,80,80;PA400,0;");
    assert_eq!(d.stats().operations, 1);
    assert_eq!(d.final_state.position, point(1., 0.));
    assert_eq!(d.diagnostics[0].code, "COORDINATE_RANGE");
}

#[test]
fn chord_modes_change_tessellation_without_changing_analytic_curve() {
    let d = read("SP1;CI400,90;CT1;CI400,1;CT;CI400,270;");
    assert_eq!(d.pages[0].operations[0].geometry.points().len(), 5);
    assert!(d.pages[0].operations[1].geometry.points().len() > 40);
    assert_eq!(d.pages[0].operations[2].geometry.points().len(), 5);
    assert_eq!(
        partial("CT1;CI40,0;").diagnostics[0].code,
        "INVALID_ARGUMENTS"
    );
}

#[test]
fn curve_segment_limit_is_fatal_in_tolerant_mode() {
    let mut o = InterpretOptions::default();
    o.parse.strict = false;
    o.geometry_limits.max_curve_segments = 4;
    for s in ["CI40;", "SP1;PD;BZ0,400,400,400,400,0;"] {
        assert_eq!(
            parse(s.as_bytes(), o).unwrap_err().diagnostic.code,
            "LIMIT_EXCEEDED"
        );
    }
}

#[test]
fn rotation_maps_ellipse_axes_and_hatch_direction_once() {
    let options = InterpretOptions {
        profile: ezhpgl_core::DeviceProfile {
            plot_size_mm: Some([100., 50.]),
            ..Default::default()
        },
        ..Default::default()
    };
    let d = parse(
        b"RO90;IP;SC0,10,0,10;SP1;PA5,5;CI2,90;FT3,1,0;RA9,9;",
        options,
    )
    .unwrap();
    let Curve::EllipticArc {
        center,
        axis_u,
        axis_v,
        ..
    } = curve(&d, 0)
    else {
        panic!()
    };
    near_point(*center, point(50., 25.));
    near_point(*axis_u, point(0., 10.));
    near_point(*axis_v, point(-20., 0.));
    let f = polygon(&d, 1).1.unwrap();
    assert_eq!(f.angle_deg, 90.);
    assert_eq!(f.spacing_mm, Some(5.));
    for line in &f.hatch_lines {
        near(line[0].x, line[1].x);
    }
}

#[test]
fn colors_widths_and_palette_are_snapshots_and_df_preserves_them() {
    let d = read("NP3;SP5;PC1,300,-10,128;PW2,1;PD40,0;PC1,0,255,0;PW1;DF;PD80,0;IN;PD120,0;");
    let ops = &d.pages[0].operations;
    assert_eq!(ops[0].style.pen, 1);
    assert_eq!(ops[0].style.color, "#ff0080");
    assert_eq!(ops[0].style.width_mm, 2.);
    assert_eq!(ops[1].style.color, "#00ff00");
    assert_eq!(ops[1].style.width_mm, 1.);
    assert_eq!(ops[2].style.color, "#000000");
    assert_eq!(ops[2].style.width_mm, 0.35);
}

#[test]
fn relative_width_uses_effective_diagonal_and_wu_resets_widths() {
    let d = read("IP0,0,1200,1600;SP1;WU1;PW2;PD40,0;WU0;PD80,0;PW0;PD120,0;");
    let ops = &d.pages[0].operations;
    near(ops[0].style.width_mm, 1.);
    assert_eq!(ops[1].style.width_mm, 0.35);
    assert_eq!(ops[2].style.width_mm, 0.025);
    let d = read("IP0,0,800,400;SC0,10,0,10,1;WU1;PW10;SP1;PD1,1;");
    near(d.pages[0].operations[0].style.width_mm, 2f64.sqrt());
    assert_eq!(
        partial("WU1;SP1;PD40,0;").diagnostics[0].code,
        "MISSING_DEVICE_BOUNDS"
    );
}

#[test]
fn palette_shrink_drops_removed_entries_and_zero_never_draws() {
    let d = read("NP8;PC8,255,0,0;PW2,8;NP2;NP8;SP8;PD40,0;SP0;CI40;RA80,80;");
    assert_eq!(d.stats().operations, 1);
    assert_eq!(d.pages[0].operations[0].style.color, "#000000");
    assert_eq!(d.pages[0].operations[0].style.width_mm, 0.35);
}

#[test]
fn line_phase_continues_over_commands_and_returning_pen_up_travel() {
    let d = read("SP1;LT2,4,1;PD40,0;PD80,0;PU0,0;PA80,0;PD120,0;PU160,0;PD200,0;");
    let phases: Vec<_> = d.pages[0]
        .operations
        .iter()
        .map(|op| op.style.line.as_ref().unwrap().phase_mm)
        .collect();
    assert_eq!(phases, vec![0., 1., 2., 0.]);
    let svg = d.to_svg(0, 0.).unwrap();
    assert!(svg.contains("stroke-dasharray=\"2 2 \""));
    assert!(svg.contains("stroke-dashoffset=\"-2\""));
}

#[test]
fn line_restore_and_isolated_shapes_preserve_residue() {
    let d = read("SP1;LT2,4,1;PD40,0;LT;LT99;PD80,0;CI40;EA120,40;PD120,0;");
    let ops = &d.pages[0].operations;
    assert_eq!(ops[1].style.line.as_ref().unwrap().phase_mm, 1.);
    assert_eq!(ops[4].style.line.as_ref().unwrap().phase_mm, 2.);
    let d = read("SP1;LT2,4,1;PD40,0;LT;PU80,0;LT99;PD120,0;");
    assert!(d.pages[0].operations[1].style.line.is_none());
}

#[test]
fn line_attributes_adaptive_patterns_dots_and_relative_lengths_export() {
    let d = read("IP0,0,1200,1600;SP1;LT3,10;LA1,4,2,5,3,8;PD400,0;LT-2,3,1;PD600,0;LT0;PD640,0;");
    let ops = &d.pages[0].operations;
    let l = ops[0].style.line.as_ref().unwrap();
    assert_eq!(l.pattern_length_mm, Some(5.));
    assert_eq!((l.cap, l.join, l.miter_limit), (4, 5, 8.));
    let s = d.to_svg(0, 0.).unwrap();
    assert!(s.contains("stroke-linecap=\"round\""));
    assert!(s.contains("stroke-linejoin=\"bevel\""));
    assert!(s.contains("stroke-dasharray=\"0.625 1.25 0.625 0 \""));
    assert!(s.contains("M16 0 L16.025 0"));
    for cmd in ["LA1,3;", "LA2,3;", "FT10,50;"] {
        assert_eq!(partial(cmd).diagnostics[0].code, "UNSUPPORTED_ATTRIBUTE");
    }
}

const TWO_SQUARES: &str =
    "SP1;PM0;PD400,0,400,400,0,400,0,0;PM1;PU120,120;PD280,120,280,280,120,280,120,120;PM2;";

#[test]
fn polygon_holes_reuse_and_style_at_paint_time() {
    let d = read(&format!("{TWO_SQUARES}PC1,255,0,0;FP;PC1,0,0,255;EP;FP1;"));
    let (c, f) = polygon(&d, 0);
    assert_eq!(c.len(), 2);
    assert_eq!(f.unwrap().rule, "evenodd");
    assert_eq!(polygon(&d, 2).1.unwrap().rule, "nonzero");
    assert_eq!(d.pages[0].operations[0].style.color, "#ff0000");
    assert_eq!(d.pages[0].operations[1].style.color, "#0000ff");
    assert_eq!(d.final_state.position, point(3., 3.));
    assert!(!d.final_state.polygon_mode);
    let s = d.to_svg(0, 0.).unwrap();
    assert!(s.contains("fill-rule=\"evenodd\""));
    assert!(s.contains("fill-rule=\"nonzero\""));
}

#[test]
fn polygon_pen_up_edges_fill_but_do_not_edge_and_closure_moves_cursor() {
    let d = read("SP1;PM0;PD400,0;PU400,400;PD0,400;PM2;EP;FP;");
    let (c, _) = polygon(&d, 0);
    assert_eq!(c[0].points.len(), 5);
    assert_eq!(c[0].edges.iter().filter(|e| e.pen_down).count(), 2);
    assert_eq!(d.final_state.position, point(0., 0.));
    let s = d.to_svg(0, 0.).unwrap();
    assert!(s.contains("d=\"M0 0 L10 0\""));
    assert!(s.contains("d=\"M10 10 L0 10\""));
    assert!(!s.contains("d=\"M10 0 L10 10\""));
}

#[test]
fn polygon_circle_contours_preserve_analytic_sources_and_original_buffer() {
    let d = read("SP1;PA400,400;PM0;CI200,90;CI80,90;PM2;FP;PM0;PD40,40;PM2;");
    let (c, _) = polygon(&d, 0);
    assert_eq!(c.len(), 3); // initial singleton is retained but ignored in paint
    assert_eq!(c[1].edges[0].source.command, "CI");
    assert!(c[1].edges[0].curve.is_some());
    assert_eq!(c[1].points.len(), 5);
    assert_eq!(c[2].points.len(), 5);
    assert_eq!(c[1].points.first(), c[1].points.last());
}

#[test]
fn curve_after_pm1_keeps_initial_segment_and_pe_pen_is_ignored() {
    let d = read("SP1;PM0;PD40,0,0,0;PM1;BZ0,40,40,40,40,0;PM2;EP;");
    let (c, _) = polygon(&d, 0);
    assert_eq!(c[1].points[0], point(0., 0.));
    assert!(c[1].edges[0].curve.is_some());
    // Base32 final digit 95 + 2*2 selects pen 2 outside polygon mode.
    let d = read("SP1;PM0;PE7:c;PD40,0,40,40;PM2;EP;");
    assert_eq!(d.final_state.pen, 1);
}

#[test]
fn rectangles_and_wedges_replace_buffer_preserve_position_and_pen_state() {
    let d = read("SP1;PA40,80;ER80,120;FP;WG80,0,90,90;EP;");
    assert_eq!(d.stats().operations, 4);
    assert_eq!(d.final_state.position, point(1., 2.));
    assert!(!d.final_state.pen_down);
    assert_eq!(
        polygon(&d, 0).0[0].points,
        vec![
            point(1., 2.),
            point(3., 2.),
            point(3., 5.),
            point(1., 5.),
            point(1., 2.)
        ]
    );
    let c = &polygon(&d, 2).0[0];
    assert_eq!(c.points.len(), 4);
    near_point(c.points[2], point(1., 4.));
    assert_eq!(c.edges.iter().filter(|e| e.curve.is_some()).count(), 1);
    let d = read("SP1;EW40,0,360,90;");
    let c = &polygon(&d, 0).0[0];
    assert!(!c.points.contains(&point(0., 0.)));
}

#[test]
fn degenerate_fills_have_stroke_and_dot_and_nonzero_viewport() {
    let d = read("SP1;RA400,0;RA0,0;CI0;");
    let s = d.to_svg(0, 0.).unwrap();
    assert!(s.contains("cx=\"0\" cy=\"0\""));
    assert!(s.contains("L10 0"));
    assert!(!s.contains("height=\"0mm\""));
    let d = read("SP1;PM0;CI0;PM2;EP;");
    assert!(d.to_svg(0, 0.).unwrap().contains("<circle "));
}

#[test]
fn hatch_holes_evenodd_and_nonzero_have_distinct_intersections() {
    let d = read(&format!("{TWO_SQUARES}FT3,40,0;FP;FP1;"));
    let f = polygon(&d, 0).1.unwrap();
    assert_eq!(f.spacing_mm, Some(1.));
    let at5: Vec<_> = f
        .hatch_lines
        .iter()
        .filter(|line| line[0].y == 5.)
        .collect();
    assert_eq!(
        at5,
        vec![
            &[point(0., 5.), point(3., 5.)],
            &[point(7., 5.), point(10., 5.)]
        ]
    );
    let at5: Vec<_> = polygon(&d, 1)
        .1
        .unwrap()
        .hatch_lines
        .iter()
        .filter(|line| line[0].y == 5.)
        .collect();
    assert_eq!(at5, vec![&[point(0., 5.), point(10., 5.)]]);
}

#[test]
fn hatch_winding_holes_respect_opposite_direction() {
    let d=read("SP1;PM0;PD400,0,400,400,0,400,0,0;PM1;PU120,120;PD120,280,280,280,280,120,120,120;PM2;FT3,40;FP1;");
    assert_eq!(
        polygon(&d, 0)
            .1
            .unwrap()
            .hatch_lines
            .iter()
            .filter(|line| line[0].y == 5.)
            .count(),
        2
    );
}

#[test]
fn hatch_spacing_tracks_scaling_freezes_and_remembers_each_type() {
    let d=read("IP0,0,400,400;SC0,10,0,10;FT3,2,30;FT4,4,0;SC0,20,0,20;SP1;RA20,20;FT3;RA20,20;SC;IP0,0,800,800;RA400,400;");
    assert_eq!(polygon(&d, 0).1.unwrap().spacing_mm, Some(2.));
    assert_eq!(polygon(&d, 1).1.unwrap().spacing_mm, Some(1.));
    assert_eq!(polygon(&d, 2).1.unwrap().spacing_mm, Some(1.));
    assert_eq!(polygon(&d, 1).1.unwrap().angle_deg, 30.);
}

#[test]
fn hatch_patterns_anchor_and_stagger_fixed_but_not_adaptive_lines() {
    let d = read("SP1;LT2,4,1;FT3,40;RA400,400;LT-2,4,1;RA400,400;");
    let f = polygon(&d, 0).1.unwrap();
    assert_eq!(&f.hatch_phases_mm[..3], &[0., 2., 0.]);
    assert!(polygon(&d, 1).1.unwrap().hatch_phases_mm.is_empty());
    assert!(d
        .to_svg(0, 0.)
        .unwrap()
        .contains("stroke-dashoffset=\"-2\""));
}

#[test]
fn clipped_curves_and_fills_keep_unclipped_data_and_paint_order() {
    let d = read("SP1;IW0,0,400,400;PA200,200;PC1,255,0,0;CI400;PC1,0,0,255;RA800,800;");
    assert_eq!(d.pages[0].operations[0].geometry.points().len(), 73);
    assert_eq!(
        d.pages[0].operations[1].visible_bounds().unwrap().max,
        point(10., 10.)
    );
    let s = d.to_svg(0, 0.).unwrap();
    assert!(s.find("#ff0000").unwrap() < s.find("#0000ff").unwrap());
    assert_eq!(s.matches("<clipPath ").count(), 2);
}

#[test]
fn unclosed_polygon_and_forbidden_commands_stop_without_painting() {
    let d = partial("SP1;PD40,0;PM0;PD80,0,80,40;");
    assert_eq!(d.diagnostics[0].code, "UNCLOSED_POLYGON");
    assert!(d.final_state.polygon_mode);
    assert_eq!(d.stats().operations, 1);
    let d = partial("PM0;PD40,0;SC0,1,0,1;PM2;FP;");
    assert_eq!(d.diagnostics[0].code, "INVALID_POLYGON_COMMAND");
    assert_eq!(d.stats().operations, 0);
    let d = read("SP1;PM0;PD40,0,40,40;EP;FP;IN;");
    assert!(!d.final_state.polygon_mode);
    assert_eq!(d.stats().operations, 0);
}

#[test]
fn polygon_hatch_limits_are_fatal_and_svg_expansion_is_bounded() {
    let cases = [
        (
            "PM0;PD40,0,40,40;PM2;",
            GeometryLimits {
                max_polygon_vertices: 3,
                ..Default::default()
            },
        ),
        (
            "PM0;PM1;PA40,40;",
            GeometryLimits {
                max_polygon_contours: 1,
                ..Default::default()
            },
        ),
        (
            "SP1;FT3,1;RA400,400;",
            GeometryLimits {
                max_hatch_lines: 5,
                ..Default::default()
            },
        ),
    ];
    for (s, geometry_limits) in cases {
        let mut options = InterpretOptions {
            geometry_limits,
            ..Default::default()
        };
        options.parse.strict = false;
        assert_eq!(
            parse(s.as_bytes(), options).unwrap_err().diagnostic.code,
            "LIMIT_EXCEEDED"
        );
    }
    let d = read("SP1;LT1,.000001,1;PD4000,0;");
    assert!(matches!(
        d.to_svg(0, 0.),
        Err(ezhpgl_core::ExportError::RenderLimit)
    ));
}
