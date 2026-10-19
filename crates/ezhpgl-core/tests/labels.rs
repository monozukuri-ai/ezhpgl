use ezhpgl_core::{
    parse, DeviceProfile, Geometry, GeometryLimits, InterpretOptions, Label, ParseOptions,
    PlotDocument, Point,
};

fn read(data: &[u8]) -> PlotDocument {
    parse(data, InterpretOptions::default()).unwrap()
}
fn tolerant() -> InterpretOptions {
    InterpretOptions {
        parse: ParseOptions {
            strict: false,
            ..Default::default()
        },
        preserve_commands: true,
        ..Default::default()
    }
}
fn label(d: &PlotDocument, index: usize) -> &Label {
    let Geometry::Label { label } = &d.pages[0].operations[index].geometry else {
        panic!("expected label")
    };
    label
}
fn near(p: Point, x: f64, y: f64) {
    assert!(
        (p.x - x).abs() < 1e-9 && (p.y - y).abs() < 1e-9,
        "{p:?} != ({x},{y})"
    );
}

#[test]
fn label_bytes_glyphs_spans_and_nonfatal_approximation() {
    let data = b"SP1;SI.2,.3;PA40,80;LBHi\x03;PD400,80;";
    let d = parse(
        data,
        InterpretOptions {
            preserve_commands: true,
            ..Default::default()
        },
    )
    .unwrap();
    let l = label(&d, 0);
    assert_eq!(l.data, b"Hi");
    assert_eq!(l.decoded, "Hi");
    assert_eq!(l.terminator, 3);
    assert_eq!(&data[l.payload_span.start..l.payload_span.end], b"Hi");
    assert_eq!(l.glyphs[1].source_offset, l.payload_span.start + 1);
    near(l.start, 1., 2.);
    near(l.end, 7., 2.);
    near(l.glyphs[1].origin, 4., 2.);
    near(l.glyphs[0].axis_x, 2., 0.);
    near(l.glyphs[0].axis_y, 0., 3.);
    assert!(d.pages[0].operations.iter().all(|op| op.approximate));
    assert_eq!(d.completeness, "partial");
    assert_eq!(d.diagnostics[0].code, "TEXT_APPROXIMATION");
    assert_eq!(
        d.interpreted_commands,
        d.raw.as_ref().unwrap().commands.len()
    );
    assert!(!d.final_state.position_approximate);
    assert!(d.final_state.pen_down);
}

#[test]
fn relative_text_size_direction_and_scaling_have_distinct_roles() {
    let d = read(b"SP1;IP0,0,4000,2000;SC0,10,0,10;PA0,0;SR2,6;DR1,1;LBA\x03");
    let g = &label(&d, 0).glyphs[0];
    let n = 5f64.sqrt();
    near(g.axis_x, 4. / n, 2. / n);
    near(g.axis_y, -3. / n, 6. / n);
    near(label(&d, 0).end, 6. / n, 3. / n);
    let d = read(b"SP1;IP0,0,4000,2000;SC0,1,0,1;SI.2,.3;DI0,1;LBA\x03");
    near(label(&d, 0).glyphs[0].axis_x, 0., 2.);
    near(label(&d, 0).end, 0., 3.);
}

#[test]
fn relative_size_requires_known_extent_and_tracks_new_points() {
    assert_eq!(
        parse(b"SP1;SR;LBA\x03", InterpretOptions::default())
            .unwrap_err()
            .diagnostic
            .code,
        "MISSING_DEVICE_BOUNDS"
    );
    let d = parse(
        b"SP1;SR;LBA\x03IP0,0,8000,4000;LBB\x03",
        InterpretOptions {
            profile: DeviceProfile {
                plot_size_mm: Some([100., 50.]),
                paper_size_mm: None,
            },
            ..Default::default()
        },
    )
    .unwrap();
    near(label(&d, 0).glyphs[0].axis_x, 0.75, 0.);
    near(label(&d, 1).glyphs[0].axis_x, 1.5, 0.);
}

#[test]
fn rotation_slant_and_negative_sizes_preserve_signed_axes() {
    let d = parse(
        b"SP1;IP0,0,4000,2000;RO90;PA0,0;SI-.2,.3;SL.5;LBA\x03",
        InterpretOptions {
            profile: DeviceProfile {
                plot_size_mm: Some([100., 50.]),
                paper_size_mm: None,
            },
            ..Default::default()
        },
    )
    .unwrap();
    let l = label(&d, 0);
    near(l.glyphs[0].axis_x, 0., -2.);
    near(l.glyphs[0].axis_y, -3., 1.5);
    near(l.glyphs[0].origin, l.start.x, l.start.y);
    near(l.end, l.start.x, l.start.y - 3.);
    let d = read(b"SP1;SI.2,-.3;LBA\x03");
    near(label(&d, 0).glyphs[0].origin, 0., 0.);
    near(label(&d, 0).glyphs[0].axis_y, 0., -3.);
}

#[test]
fn origins_align_caps_and_restore_or_concatenate_per_path() {
    for origin in (1..=9).chain(11..=19) {
        let d = read(format!("SP1;SI.2,.3;LO{origin};LBAB\x03").as_bytes());
        let l = label(&d, 0);
        let o = if origin > 10 { origin - 10 } else { origin };
        let col = (o - 1) / 3;
        let row = (o - 1) % 3;
        let offset = |v| {
            if origin <= 10 {
                0.
            } else {
                match v {
                    0 => 1.5,
                    2 => -1.5,
                    _ => 0.,
                }
            }
        };
        near(
            l.glyphs[0].origin,
            -2.5 * f64::from(col) + offset(col),
            -1.5 * f64::from(row) + offset(row),
        );
        near(l.end, if o <= 3 { 6. } else { 0. }, 0.);
    }
    for (path, lo, x, y) in [
        (0, 1, 6., 0.),
        (1, 3, 0., -12.),
        (2, 7, -6., 0.),
        (3, 1, 0., 12.),
    ] {
        let d = read(format!("SP1;SI.2,.3;DV{path};LO{lo};LBAB\x03").as_bytes());
        near(label(&d, 0).end, x, y);
    }
    let d = read(b"SP1;SI.2,.3;LO5;LBAB\x03LBC\x03");
    near(label(&d, 1).start, 0., 0.);
    assert!(!d.final_state.position_approximate);
}

#[test]
fn cr_lf_tab_backspace_and_cp_keep_carriage_anchor() {
    let d = read(b"SP1;SI.2,.3;PA40,80;LBA\x08B\tC\rD\nE\x03CP;LBF\x03");
    let l = label(&d, 0);
    for (g, x, y) in [
        (0, 1., 2.),
        (1, 1., 2.),
        (2, 25., 2.),
        (3, 1., 2.),
        (4, 4., -4.),
    ] {
        near(l.glyphs[g].origin, x, y);
    }
    near(label(&d, 1).start, 1., -10.);
    near(d.final_state.position, 4., -10.);
    let d = read(b"SP1;SI.2,.3;LB\tA\x03CP2,1;");
    near(label(&d, 0).glyphs[0].origin, 24., 0.);
    near(d.final_state.position, 33., 6.);
}

#[test]
fn spacing_and_reversed_line_direction_change_advances() {
    let d = read(b"SP1;SI.2,.3;ES.5,.5;DV0,1;LBA\nB\x03");
    near(label(&d, 0).glyphs[1].origin, 4.5, 9.);
    near(d.final_state.position, 9., 9.);
}

#[test]
fn sd_ad_and_shift_controls_snapshot_each_glyph() {
    let d = read(b"SP1;SD1,14,3,10,4,12,5,1,6,3,7,4099;AD1,277,3,20,4,6;LBA\x0e\xc5\x0f\xe9\x03");
    let l = label(&d, 0);
    assert_eq!(l.decoded, "A\u{e}é\u{f}é");
    assert_eq!(l.glyphs[0].font.typeface, 4099);
    assert_eq!(l.glyphs[0].font.weight, 3);
    assert_eq!(l.glyphs[1].font.pitch, 20.);
    assert_eq!(l.glyphs[2].font.pitch, 10.);
    near(l.glyphs[0].axis_x, 25.4 / 10. * 2. / 3., 0.);
    assert!(!d.final_state.text.unwrap().use_alternate);
}

#[test]
fn single_byte_decoding_raw_retention_and_transparent_controls() {
    for (cmd, bytes, decoded) in [
        ("SD1,14;", b"\xe9\xff".as_slice(), "éÿ"),
        ("SD1,277;", b"\xc5".as_slice(), "é"),
        ("SD1,341;TD1;", b"\x82".as_slice(), "é"),
        ("SD1,21;", b"\xe9".as_slice(), "�"),
    ] {
        let mut data = format!("SP1;{cmd}LB").into_bytes();
        data.extend_from_slice(bytes);
        data.push(3);
        let d = read(&data);
        assert_eq!(label(&d, 0).decoded, decoded);
        assert_eq!(label(&d, 0).data, bytes);
    }
    let d = read(b"SP1;TD1;DT#,0;LB\x00\n#");
    let l = label(&d, 0);
    assert_eq!(l.decoded, "��#");
    assert_eq!(l.glyphs.len(), 3);
    assert_eq!(l.unmapped_offsets.len(), 2);
    assert_eq!(l.glyphs[2].source_offset, l.payload_span.end);
    assert!(l.print_terminator);
    assert_eq!(d.diagnostics.len(), 2);
}

#[test]
fn font_defaults_selection_and_size_reset_are_explicit() {
    let d = read(b"SP1;AD3,20;SA;SI.2,.3;LBA\x03SI;LBB\x03SS;SD;LBC\x03");
    assert_eq!(label(&d, 0).glyphs[0].font.pitch, 20.);
    near(label(&d, 1).glyphs[0].axis_x, 25.4 / 20. * 2. / 3., 0.);
    assert_eq!(label(&d, 2).glyphs[0].font.pitch, 9.);
    let d = read(b"SP1;SI.2,.3;LBA\x03DF;LBB\x03IN;LBC\x03");
    near(label(&d, 1).start, 3., 0.);
    near(label(&d, 2).start, 0., 0.);
    assert_eq!(label(&d, 1).state.size_mode, "font");
}

#[test]
fn text_pen_state_line_attributes_and_path_order_are_preserved() {
    let d = read(b"SP1;LT2,5,1;SI.2,.3;PA0,0;PD40,0;LBA\x03PR40,0;PU;LBB\x03SP0;LBC\x03");
    assert!(label(&d, 1).approximate);
    assert!(d.pages[0].operations[1].style.line.is_none());
    assert!(d.pages[0].operations[2].style.line.is_some());
    assert!(!d.final_state.pen_down);
    assert_eq!(d.stats().operations, 4);
    near(d.final_state.position, 11., 0.);
}

#[test]
fn dependency_quality_clears_only_after_absolute_positioning_or_reset() {
    let d = read(b"SP1;SI.2,.3;LBA\x03PD;PR40,0;PA400,0;PD800,0;PU;CP1,0;CI10;PA0,0;CI10;");
    let ops = &d.pages[0].operations;
    assert!(ops[1].approximate && ops[2].approximate && ops[3].approximate);
    assert!(!ops[4].approximate);
    assert!(ops[5].approximate);
    assert!(!ops[6].approximate);
    assert!(!d.final_state.position_approximate);
    let d = read(b"SP1;SI.2,.3;LBA\x03PM0;PD400,0,400,400;PM2;PU0,0;FP;");
    assert!(d.pages[0].operations.last().unwrap().approximate);
    assert!(!d.final_state.position_approximate);
}

#[test]
fn unsupported_text_settings_stop_before_dependent_geometry() {
    // FI/FN are ignored with a warning since 0.1.0a2; the others still stop
    for cmd in ["LO21;", "SD2,1;", "AD1,999;", "SB1;", "CF1;", "ES-1;"] {
        let input = format!("SP1;PD40,0;{cmd}LBabc\x03PD80,0;");
        assert!(parse(input.as_bytes(), InterpretOptions::default()).is_err());
        let d = parse(input.as_bytes(), tolerant()).unwrap();
        assert_eq!(d.stats().operations, 1, "{cmd}");
        near(d.final_state.position, 1., 0.);
    }
}

#[test]
fn invalid_text_and_embedded_reset_are_atomic() {
    for cmd in [
        "SI1;",
        "DI0,0;",
        "SD3,-1;",
        "DV4;",
        "CP1;",
        "LBok\x1bEbad\x03",
        "SI32767,32767;LBx\x03",
    ] {
        let d = parse(format!("SP1;PD40,0;{cmd}PD80,0;").as_bytes(), tolerant()).unwrap();
        assert_eq!(d.stats().operations, 1, "{cmd}");
        near(d.final_state.position, 1., 0.);
    }
    let d = read(b"SP1;TD1;LB\x1bE\x03");
    assert_eq!(label(&d, 0).glyphs.len(), 2);
}

#[test]
fn label_byte_glyph_vertex_and_diagnostic_limits_are_fatal() {
    for (input, limits) in [
        (
            b"SP1;LBabc\x03".as_slice(),
            GeometryLimits {
                max_label_bytes: 2,
                ..Default::default()
            },
        ),
        (
            b"SP0;LBA\x03LBB\x03".as_slice(),
            GeometryLimits {
                max_glyphs: 1,
                ..Default::default()
            },
        ),
        (
            b"SP1;LBA\x03".as_slice(),
            GeometryLimits {
                max_vertices: 3,
                ..Default::default()
            },
        ),
        (
            b"SP1;DT#,0;LBA#".as_slice(),
            GeometryLimits {
                max_label_bytes: 1,
                ..Default::default()
            },
        ),
    ] {
        assert_eq!(
            parse(
                input,
                InterpretOptions {
                    geometry_limits: limits,
                    ..tolerant()
                }
            )
            .unwrap_err()
            .diagnostic
            .code,
            "LIMIT_EXCEEDED"
        );
    }
    let mut o = tolerant();
    o.parse.limits.max_diagnostics = 0;
    assert_eq!(
        parse(b"LBA\x03", o).unwrap_err().diagnostic.code,
        "LIMIT_EXCEEDED"
    );
}

#[test]
fn clipping_xml_escaping_empty_labels_and_mutated_geometry_are_safe() {
    let d = read(b"SP1;SI.2,.3;IW0,0,40,40;LB<&\"'\x03LB\x03LB\r\x03");
    let svg = d.to_svg(0, 0.).unwrap();
    assert!(
        svg.contains("&lt;")
            && svg.contains("&amp;")
            && svg.contains("&quot;")
            && svg.contains("&apos;")
    );
    assert!(svg.contains("data-approximate=\"true\"") && svg.contains("clip-path="));
    assert_eq!(d.stats().operations, 2);
    assert_eq!(d.pages[0].bounds.unwrap().max, Point { x: 1., y: 1. });
    let mut d = read(b"SP1;LBA\x03");
    let Geometry::Label { label } = &mut d.pages[0].operations[0].geometry else {
        unreachable!()
    };
    label.glyphs[0].character = '\0';
    assert!(d.to_svg(0, 1.).is_err());
    assert!(d.to_json().is_err());
}
