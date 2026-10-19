use ezhpgl_core::{
    parse, CoordinateMode, DeviceProfile, GeometryLimits, InterpretOptions, PlotDocument, Point,
};

fn p(x: f64, y: f64) -> Point {
    Point { x, y }
}
fn read(s: &[u8]) -> PlotDocument {
    parse(s, InterpretOptions::default()).unwrap()
}
fn tolerant() -> InterpretOptions {
    let mut o = InterpretOptions::default();
    o.parse.strict = false;
    o
}

#[test]
fn hand_encoded_base64_signed_magnitude_and_multiple_digits() {
    // Literal wire values: 40 = [79,192], -40 = [80,192], 0 = 191.
    let d = read(b"SP1;PE\x4f\xc0\xbf\xbf\x4f\xc0\x50\xc0\xbf;");
    assert_eq!(
        d.pages[0].operations[0].geometry.points(),
        &[p(0., 0.), p(1., 0.), p(1., 1.), p(0., 1.)]
    );
    assert_eq!(d.stats().operations, 1);
    assert_eq!(d.stats().vertices, 4);
    assert_eq!(d.pages[0].operations[0].source.command, "PE");
    // Published base-64 digits [121,71,196] encode the integer 10525.
    let d = read(b"SP1;PE\x79\x47\xc4\xbf;");
    assert_eq!(d.final_state.position, p(10525. / 40., 0.));
    // Signed-magnitude negative zero, not zigzag -1.
    assert_eq!(read(b"PE\xc0\xc0;").final_state.position, p(0., 0.));
}

#[test]
fn all_flags_pen_selection_and_previous_mode_restoration() {
    let d = read(b"PR;PE:\xc3<=\x4f\xc0\x4f\xc0\x4f\xc0\xbf:\xc5\xbf\x4f\xc0;PD40,0;");
    let ops = &d.pages[0].operations;
    assert_eq!(ops[0].style.pen, 2);
    assert_eq!(ops[0].geometry.points(), &[p(1., 1.), p(2., 1.)]);
    assert_eq!(ops[1].style.pen, 3);
    assert_eq!(ops[1].geometry.points(), &[p(2., 1.), p(2., 2.)]);
    assert_eq!(d.final_state.coordinate_mode, CoordinateMode::Relative);
    assert_eq!(d.final_state.position, p(3., 2.));
    assert_eq!(d.final_state.pen, 3);
    assert!(d.final_state.pen_down);
    let d = read(b"PA400,800;PE\x4f\xc0\xbf;PD40,80;");
    assert_eq!(d.final_state.position, p(1., 2.)); // PE relative, then prior PA mode
    assert_eq!(d.final_state.coordinate_mode, CoordinateMode::Absolute);
}

#[test]
fn pen_up_and_absolute_flags_are_one_shot() {
    let d = read(b"SP1;PA400,400;PE<=\x4f\xc0\xbf\x4f\xc0\xbf<\xbf\x4f\xc0;PR40,0;");
    assert_eq!(
        d.pages[0].operations[0].geometry.points(),
        &[p(1., 0.), p(2., 0.)]
    );
    assert_eq!(d.stats().operations, 1);
    assert_eq!(d.final_state.position, p(3., 1.));
    assert!(!d.final_state.pen_down);
}

#[test]
fn deselected_pen_moves_and_flag_only_or_empty_pe_keeps_pen_state() {
    let d = read(b"PE\x4f\xc0\xbf;PE:\xc1;PE;PD40,40;");
    assert_eq!(d.stats().operations, 1);
    assert_eq!(
        d.pages[0].operations[0].geometry.points(),
        &[p(1., 0.), p(1., 1.)]
    );
    assert!(d.final_state.pen_down);
    let d = read(b"SP1;PE7>c;PE;PA40,40;");
    assert!(!d.final_state.pen_down);
    assert_eq!(d.stats().operations, 0);
}

#[test]
fn fraction_bits_apply_to_coordinates_not_flags_and_reset_each_pe() {
    // Fraction 1 => encoded 80 is 40; pen 2 stays 2, fraction flag is signed too.
    let d = read(b"PE>\xc1:\xc3\x5f\xc1\xbf;PE\x4f\xc0\xbf;");
    assert_eq!(d.final_state.position, p(2., 0.));
    assert_eq!(d.final_state.pen, 2);
    // Fraction -1 => decoded 20 becomes 40.
    let d = read(b"SP1;PE>\xc2\xe7\xbf;");
    assert_eq!(d.final_state.position, p(1., 0.));
    let d = read(b"PE>\xf3\xc1\xbf;"); // +26 => 2^-26 plotter units
    assert_eq!(d.final_state.position.x, 2f64.powi(-26) / 40.);
}

#[test]
fn base32_parity_flags_and_ignored_bytes_inside_numbers() {
    // Base32: 40 => [79,97], -40 => [80,97]. Eighth bit ignored.
    let d = read(b"SP1;PE7\xcf\xe1\xdf\xdf\xd0\xe1;");
    assert_eq!(d.final_state.position, p(1., -1.));
    let d = read(b"SP1;PE\xbd\xbc\x4f\x00\x20\x7f\x80\xa0\xff\xc0\xbf\x4f\xc0\xbf;");
    assert_eq!(
        d.pages[0].operations[0].geometry.points(),
        &[p(1., 0.), p(2., 0.)]
    );
    assert_eq!(read(b"PE7Oa_;PEO\xc0\xbf;").final_state.position, p(2., 0.));
}

#[test]
fn compressed_moves_share_scale_rotation_and_clip_with_plain_coordinates() {
    let options = InterpretOptions {
        profile: DeviceProfile {
            plot_size_mm: Some([100., 50.]),
            ..Default::default()
        },
        ..Default::default()
    };
    let prefix = b"SP1;RO90;IP;SC0,10,0,10;IW2,2,8,8;";
    let mut encoded = prefix.to_vec();
    encoded.extend_from_slice(b"PE<=\xbf\xc9\xd3\xbf;"); // abs(0,5), rel(10,0)
    let d = parse(&encoded, options).unwrap();
    let plain = parse(&[prefix.as_slice(), b"PU0,5;PR;PD10,0;"].concat(), options).unwrap();
    assert_eq!(d.pages[0].bounds, plain.pages[0].bounds);
    let op = &d.pages[0].operations[0];
    assert_eq!(op.geometry, plain.pages[0].operations[0].geometry);
    assert_eq!(op.clip, plain.pages[0].operations[0].clip);
    assert_eq!(op.visible_bounds().unwrap().min, p(50., 10.));
    assert_eq!(op.visible_bounds().unwrap().max, p(50., 40.));
}

#[test]
fn malformed_pe_is_atomic_including_pen_mode_position_and_full_source_span() {
    for (body, code) in [
        (b"\xc1".as_slice(), "PE_TRUNCATED"),
        (b"?", "PE_TRUNCATED"),
        (b"<", "PE_TRUNCATED"),
        (b"=", "PE_TRUNCATED"),
        (b"\xc1=\xc1", "PE_INVALID_BYTE"),
        (b"!", "PE_INVALID_BYTE"),
        (b">\xf5", "PE_FRACTION_RANGE"),
        (b":\xc2", "INVALID_PEN"),
        (b"????????????\xbf\xbf", "PE_OVERFLOW"),
    ] {
        let prefix = b"SP1;PA40,80;PE:\xc5\x4f\xc0\xbf";
        let source = [prefix.as_slice(), body, b";PD400,400;"].concat();
        let d = parse(&source, tolerant()).unwrap();
        assert_eq!(d.completeness, "partial");
        assert_eq!(d.diagnostics[0].code, code, "{body:?}");
        assert_eq!(d.stats().operations, 0);
        assert_eq!(d.final_state.position, p(1., 2.));
        assert_eq!(d.final_state.pen, 1);
        assert!(!d.final_state.pen_down);
        assert_eq!(d.final_state.coordinate_mode, CoordinateMode::Absolute);
        let span = d.diagnostics[0].span;
        assert!(span.start >= prefix.len());
        assert!(span.end <= prefix.len() + body.len());
        assert!(parse(&source, InterpretOptions::default()).is_err());
    }
}

#[test]
fn decoded_value_limits_include_hidden_moves_and_flag_values() {
    for source in [b"PE<\xbf\xbf;".as_slice(), b"PE\xbf\xbf;", b"PE:\xc1>\xbf;"] {
        let mut opts = tolerant();
        opts.parse.limits.max_arguments_per_command = 1;
        assert_eq!(
            parse(source, opts).unwrap_err().diagnostic.code,
            "LIMIT_EXCEEDED"
        );
    }
    let options = InterpretOptions {
        geometry_limits: GeometryLimits {
            max_vertices: 2,
            ..Default::default()
        },
        ..tolerant()
    };
    assert_eq!(
        parse(b"SP1;PE\xc1\xc1\xc1\xc1;", options)
            .unwrap_err()
            .diagnostic
            .code,
        "LIMIT_EXCEEDED"
    );
    let options = InterpretOptions {
        geometry_limits: GeometryLimits {
            max_operations: 1,
            ..Default::default()
        },
        ..tolerant()
    };
    assert_eq!(
        parse(b"SP1;PE\xc1\xc1<\xc1\xc1\xc1\xc1;", options)
            .unwrap_err()
            .diagnostic
            .code,
        "LIMIT_EXCEEDED"
    );
}

#[test]
fn encoded_coordinate_overflow_does_not_commit_earlier_pairs() {
    // -26 fraction maps integer 1 to 67,108,864: outside supported plotter range.
    let d = parse(b"SP1;PE\xc1\xc1>\xf4\xc1\xbf;", tolerant()).unwrap();
    assert_eq!(d.diagnostics[0].code, "COORDINATE_RANGE");
    assert_eq!(d.stats().operations, 0);
    assert_eq!(d.final_state.position, p(0., 0.));
}
