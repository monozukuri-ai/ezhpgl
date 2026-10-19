use ezhpgl_core::{
    parse, Bounds, ClipRegion, DeviceProfile, InterpretOptions, PlotDocument, Point,
};

fn point(x: f64, y: f64) -> Point {
    Point { x, y }
}
fn bounds(x: f64, y: f64, xx: f64, yy: f64) -> Bounds {
    Bounds {
        min: point(x, y),
        max: point(xx, yy),
    }
}
fn read(s: &str) -> PlotDocument {
    parse(s.as_bytes(), InterpretOptions::default()).unwrap()
}
fn device() -> InterpretOptions {
    InterpretOptions {
        profile: DeviceProfile {
            plot_size_mm: Some([100., 50.]),
            paper_size_mm: Some([110., 60.]),
        },
        ..Default::default()
    }
}
fn profiled(s: &str) -> PlotDocument {
    parse(s.as_bytes(), device()).unwrap()
}

#[test]
fn anisotropic_coordinates_relative_vectors_and_physical_cursor() {
    let d = read("SP1;PA40,80;IP400,800,1200,2000;SC0,10,0,10;PD10,10;PR-5,-5;SC;PR40,0;");
    let ops = &d.pages[0].operations;
    assert_eq!(ops[0].geometry.points(), &[point(1., 2.), point(30., 50.)]);
    assert_eq!(
        ops[1].geometry.points(),
        &[point(30., 50.), point(20., 35.)]
    );
    assert_eq!(d.final_state.position, point(21., 35.));
    assert_eq!(d.final_state.transform.unwrap().scaling, "off");
}

#[test]
fn isotropic_alignment_both_axes_and_mirrors() {
    for (ip, sc, expected) in [
        ("IP0,0,800,400;", "SC0,10,0,10,1;", point(5., 0.)),
        ("IP0,0,800,400;", "SC0,10,0,10,1,100,0;", point(10., 0.)),
        ("IP0,0,400,800;", "SC0,10,0,10,1,0,100;", point(0., 10.)),
        ("IP0,0,400,800;", "SC0,10,0,10,1,0,-5;", point(0., 0.)),
        ("IP800,0,0,400;", "SC0,10,0,10,1;", point(15., 0.)),
        ("IP0,0,800,400;", "SC10,0,0,10,1;", point(15., 0.)),
        ("IP0,0,400,800;", "SC0,10,10,0,1;", point(0., 15.)),
        ("IP0,0,800,400;", "SC0,10,0,10,0,100,100;", point(0., 0.)),
    ] {
        let d = read(&format!("{ip}{sc}PA0,0;"));
        assert_eq!(d.final_state.position, expected, "{ip}{sc}");
    }
}

#[test]
fn point_factor_scaling_does_not_require_device_p2() {
    let d = read("SC2,40,-3,-80,2;PA3,-2;PR1,1;");
    assert_eq!(d.final_state.position, point(2., -4.));
    let t = d.final_state.transform.unwrap();
    assert_eq!(t.matrix, [1., 0., 0., -2., -2., -6.]);
    assert!(t.p2_plotter_units.is_none());
}

#[test]
fn ip_rounding_tracking_and_coincident_axes() {
    let d = read("IP-1.5,2.5,-1.5,2.5;IP10,20;");
    let t = d.final_state.transform.unwrap();
    assert_eq!(t.p1_plotter_units, point(10., 20.));
    assert_eq!(t.p2_plotter_units, Some(point(11., 21.)));
    let d = read("IP40,80,440,880;IP80,160;SC0,10,0,10;PA10,10;");
    assert_eq!(d.final_state.position, point(12., 24.));
}

#[test]
fn rotation_has_origin_shift_is_not_cumulative_and_preserves_cursor() {
    for (angle, expected) in [
        (0, point(10., 20.)),
        (90, point(80., 10.)),
        (180, point(90., 30.)),
        (270, point(20., 40.)),
    ] {
        let d = profiled(&format!("PA40,80;RO{angle};RO{angle};SP1;PD400,800;"));
        assert_eq!(
            d.pages[0].operations[0].geometry.points(),
            &[point(1., 2.), expected]
        );
        let t = d.final_state.transform.unwrap();
        assert_eq!(t.p2_plotter_units, Some(point(4000., 2000.)));
        assert_eq!(t.rotation, angle);
    }
    let d = profiled("RO90;IP;SC0,10,0,10;PA0,0;PR10,10;");
    assert_eq!(d.final_state.position, point(0., 50.));
    assert_eq!(
        d.final_state.transform.unwrap().p2_plotter_units,
        Some(point(2000., 4000.))
    );
}

#[test]
fn ir_percentages_follow_oriented_extents_and_allow_outside_points() {
    let d = profiled("RO90;IR10,20,90,80;IR20,30;SC0,10,0,10;PA10,10;");
    assert_eq!(d.final_state.position, point(10., 50.));
    let t = d.final_state.transform.unwrap();
    assert_eq!(t.p1_plotter_units, point(400., 1200.));
    assert_eq!(t.p2_plotter_units, Some(point(2000., 3600.)));
    let d = profiled("IR-10,0,110,100;SC0,10,0,10;PA0,0;");
    assert_eq!(d.final_state.position, point(-10., 0.));
}

#[test]
fn device_defaults_are_explicit_and_paper_is_only_metadata() {
    let d = profiled("IN;");
    assert_eq!(d.pages[0].plot_size_mm, Some([100., 50.]));
    assert_eq!(d.pages[0].paper_size_mm, Some([110., 60.]));
    assert_eq!(
        d.final_state.transform.unwrap().clip.unwrap().bounds(),
        Some(bounds(0., 0., 100., 50.))
    );
    assert!(read("IN;IP;IR;RO;IW;").final_state.transform.is_none());
    for s in ["RO90;", "IR0,0,100,100;", "IP0,0;", "SC0,1,0,1;"] {
        assert_eq!(
            parse(s.as_bytes(), InterpretOptions::default())
                .unwrap_err()
                .diagnostic
                .code,
            "MISSING_DEVICE_BOUNDS"
        );
    }
    let mut options = InterpretOptions {
        profile: DeviceProfile {
            paper_size_mm: Some([100., 50.]),
            ..Default::default()
        },
        ..Default::default()
    };
    assert!(parse(b"RO90;", options).is_err());
    options.parse.strict = false;
    for size in [
        [0., 1.],
        [-1., 2.],
        [f64::NAN, 1.],
        [1., f64::INFINITY],
        [300000., 1.],
    ] {
        options.profile.plot_size_mm = Some(size);
        assert_eq!(
            parse(b"", options).unwrap_err().diagnostic.code,
            "INVALID_PROFILE"
        );
    }
}

#[test]
fn df_preserves_points_rotation_cursor_and_pen_but_in_resets_space() {
    let d = profiled("SP2;PA40,80;IP400,800,1200,1600;RO90;SC0,1,0,1;IW0,0,1,1;PD;DF;");
    let t = d.final_state.transform.unwrap();
    assert_eq!(t.rotation, 90);
    assert_eq!(t.p1_plotter_units, point(400., 800.));
    assert_eq!(t.scaling, "off");
    assert_eq!(t.clip.unwrap().bounds(), Some(bounds(0., 0., 100., 50.)));
    assert_eq!(d.final_state.position, point(1., 2.));
    assert!(d.final_state.pen_down);
    let d = profiled("SP2;IP400,800,1200,1600;RO90;SC0,1,0,1;IW0,0,1,1;IN;");
    let t = d.final_state.transform.unwrap();
    assert_eq!(t.rotation, 0);
    assert_eq!(t.p1_plotter_units, point(0., 0.));
    assert_eq!(t.p2_plotter_units, Some(point(4000., 2000.)));
    assert_eq!(d.final_state.pen, 2);
    assert!(!d.final_state.pen_down);
}

#[test]
fn clip_changes_preserve_unclipped_geometry_cursor_and_per_operation_windows() {
    let d = read("SP1;IW0,0,400,400;PA-400,200;PD800,200;IW800,400,0,0;PR-800,0;IW;PD1200,200;");
    let ops = &d.pages[0].operations;
    assert_eq!(ops[0].geometry.points(), &[point(-10., 5.), point(20., 5.)]);
    assert_eq!(ops[0].visible_bounds(), Some(bounds(0., 5., 10., 5.)));
    assert_eq!(ops[1].geometry.points(), &[point(20., 5.), point(0., 5.)]);
    assert_eq!(ops[1].visible_bounds(), Some(bounds(0., 5., 20., 5.)));
    assert!(ops[2].clip.is_none());
    assert_eq!(d.final_state.position, point(30., 10.));
    let svg = d.to_svg(0, 0.).unwrap();
    assert_eq!(svg.matches("<clipPath ").count(), 2);
    assert!(svg.contains("M-10 5 L20 5"));
}

#[test]
fn user_window_tracks_ip_until_next_sc_freezes_it() {
    let d = read("IP0,0,400,400;SC0,10,0,10;IW2,2,8,8;IP400,0,800,400;SP1;PA0,5;PD10,5;SC0,100,0,100;IP800,0,1200,400;PD100,50;");
    let ops = &d.pages[0].operations;
    assert_eq!(
        ops[0].clip.unwrap().bounds(),
        Some(bounds(12., 2., 18., 8.))
    );
    assert_eq!(ops[0].clip, ops[1].clip);
    assert_eq!(d.final_state.position, point(30., 5.));
    let d = read("IW0,0,400,400;IP400,400,800,800;SC0,10,0,10;");
    assert_eq!(
        d.final_state.transform.unwrap().clip.unwrap().bounds(),
        Some(bounds(0., 0., 10., 10.))
    );
}

#[test]
fn rotated_soft_window_intersects_hard_window_and_resets() {
    let d = profiled("IW0,0,800,400;RO90;");
    assert_eq!(
        d.final_state.transform.unwrap().clip.unwrap().bounds(),
        Some(bounds(90., 0., 100., 20.))
    );
    let d = profiled("IW0,0,800,400;RO90;IW;");
    assert_eq!(
        d.final_state.transform.unwrap().clip.unwrap().bounds(),
        Some(bounds(0., 0., 100., 50.))
    );
    let d = profiled("IW-400,-400,-80,-80;SP1;PD100,100;");
    assert_eq!(d.pages[0].operations[0].clip, Some(ClipRegion::Empty));
    assert!(d.pages[0].bounds.is_none());
    assert!(!d.to_svg(0, 0.).unwrap().contains("<path "));
}

#[test]
fn zero_area_clip_is_empty_but_stroke_near_edge_is_not_discarded() {
    let d = read("IW0,0,0,400;SP1;PD0,200;");
    assert!(d.pages[0].bounds.is_none());
    assert!(d.to_svg(0, 0.).unwrap().contains("width=\"0\""));
    let d = read("IW0,0,400,400;SP1;PA-4,80;PD-4,320;");
    assert!(d.pages[0].bounds.is_none()); // centerline outside, stroke overlaps
    let svg = d.to_svg(0, 0.).unwrap();
    assert!(svg.contains("M-0.1 2 L-0.1 8"));
    assert!(svg.contains("clip-path=\"url(#clip-0)\""));
}

#[test]
fn invalid_transforms_are_atomic_and_do_not_reinterpret_later_commands() {
    for tail in [
        "IP1;",
        "IR1,2,3;",
        "RO45;",
        "IW1,2,3;",
        "SC0,0,0,1;",
        "SC0,10,0,10,1,50;",
        "SC0,0,0,1,2;",
        "SC0,.00000000000001,0,1;",
    ] {
        let mut options = device();
        options.parse.strict = false;
        let d = parse(format!("SP1;PD40,80;{tail}PA400,400;").as_bytes(), options).unwrap();
        assert_eq!(d.completeness, "partial", "{tail}");
        assert_eq!(d.final_state.position, point(1., 2.));
        assert_eq!(d.final_state.transform.as_ref().unwrap().scaling, "off");
        assert_eq!(d.stats().operations, 1);
    }
    let d = read("SC0,1000000,0,1,2;SP1;");
    assert_eq!(d.stats().operations, 0);
    let mut opts = InterpretOptions::default();
    opts.parse.strict = false;
    let d = parse(b"SC0,1000000,0,1,2;SP1;PD1,0,10,0;", opts).unwrap();
    assert_eq!(d.diagnostics[0].code, "COORDINATE_RANGE");
    assert_eq!(d.stats().operations, 0);
    assert_eq!(d.final_state.position, point(0., 0.));
}
