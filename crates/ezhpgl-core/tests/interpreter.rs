use ezhpgl_core::{
    parse, CoordinateMode, Geometry, GeometryLimits, InterpretOptions, ParseOptions, PlotDocument,
    Point,
};

fn read(data: &[u8]) -> PlotDocument {
    parse(data, InterpretOptions::default()).unwrap()
}
fn tolerant() -> InterpretOptions {
    InterpretOptions {
        parse: ParseOptions {
            strict: false,
            ..ParseOptions::default()
        },
        preserve_commands: true,
        ..InterpretOptions::default()
    }
}
fn xy(doc: &PlotDocument, page: usize, op: usize) -> Vec<(f64, f64)> {
    doc.pages[page].operations[op]
        .geometry
        .points()
        .iter()
        .map(|p| (p.x, p.y))
        .collect()
}

#[test]
fn device_control_commands_are_accepted_and_ignored_ones_are_reported() {
    // A CAD plot header: device control has no geometry and keeps the plot complete
    let d = read(
        b"BP;IN;PS23760,33640;EC;NP256;PA;LA1,4,2,4;TR0;MC0;NR;VS10;FS5;SP1;PU0,0;PD400,0;OE;IM;",
    );
    assert_eq!(d.completeness, "complete");
    assert_eq!(d.pages[0].operations.len(), 1);
    // the larger PS value is the X extent (an A1 landscape plot)
    assert_eq!(d.pages[0].plot_size_mm, Some([841., 594.]));
    // Commands that would change fills, patterns, ticks or fonts are ignored with a warning
    let d = read(b"IN;AC0,0;UL1,50,50;XT;SP1;PA0,0;PD400,0;");
    assert_eq!(d.completeness, "partial");
    assert_eq!(d.diagnostics.len(), 1);
    assert_eq!(d.diagnostics[0].code, "IGNORED_COMMAND");
    assert_eq!(d.diagnostics[0].action, "ignore_command");
    assert_eq!(d.diagnostics[0].command.as_deref(), Some("AC"));
    assert_eq!(d.pages[0].operations.len(), 1);
    // AF/AH advance the page like a parameterless PG
    let d = read(b"IN;SP1;PA0,0;PD400,0;AF;SP1;PA0,0;PD400,0;AH;");
    assert_eq!(d.pages.len(), 2);
    assert_eq!(d.final_state.position, Point { x: 0., y: 0. });
    // CR rescales the PC components to the generic 0..255 palette
    let d = read(b"IN;CR0,100;PC1,100,0,50;SP1;PA0,0;PD400,0;CR;PC2,255,255,0;SP2;PD800,0;");
    assert_eq!(d.pages[0].operations[0].style.color, "#ff0080");
    assert_eq!(d.pages[0].operations[1].style.color, "#ffff00");
    assert_eq!(
        parse(b"CR5,5;", InterpretOptions::default())
            .unwrap_err()
            .diagnostic
            .code,
        "INVALID_ARGUMENTS"
    );
}

#[test]
fn absolute_relative_and_pen_up_moves() {
    let d = read(b"INSP1PA40,80PD120,80,120,160PR-40,0,0,-40PU40,40PD-40,-40PA0,0;");
    assert_eq!(d.completeness, "complete");
    assert_eq!(xy(&d, 0, 0), [(1., 2.), (3., 2.), (3., 4.)]);
    assert_eq!(xy(&d, 0, 1), [(3., 4.), (2., 4.), (2., 3.)]);
    assert_eq!(xy(&d, 0, 2), [(3., 4.), (2., 3.)]);
    assert_eq!(xy(&d, 0, 3), [(2., 3.), (0., 0.)]);
    assert_eq!(d.final_state.position, Point { x: 0., y: 0. });
    assert_eq!(d.final_state.coordinate_mode, CoordinateMode::Absolute);
    assert!(d.final_state.pen_down);
}

#[test]
fn default_and_initialize_are_different_and_do_not_erase_drawing() {
    let d = read(b"SP3;PA40,80;PD;PR40,0;DF;PD80,40;IN;PD40,0;");
    assert_eq!(xy(&d, 0, 0), [(1., 2.)]);
    assert_eq!(xy(&d, 0, 1), [(1., 2.), (2., 2.)]);
    assert_eq!(xy(&d, 0, 2), [(2., 2.), (2., 1.)]);
    assert_eq!(xy(&d, 0, 3), [(0., 0.), (1., 0.)]);
    assert!(d.pages[0].operations.iter().all(|op| op.style.pen == 3));
    let df = read(b"SP3;PR40,80;PD;DF;");
    assert_eq!(df.final_state.position, Point { x: 1., y: 2. });
    assert_eq!(df.final_state.coordinate_mode, CoordinateMode::Absolute);
    assert!(df.final_state.pen_down);
    let init = read(b"SP3;PR40,80;PD;IN;");
    assert_eq!(init.final_state.position, Point::default());
    assert!(!init.final_state.pen_down);
    assert_eq!(init.final_state.pen, 3);
}

#[test]
fn deselected_pen_still_moves_and_styles_are_snapshots() {
    let d = read(b"PD40,40;SP2;PR40,0;SP0;PD0,40;SP3;PD40,0;SP;");
    assert_eq!(d.stats().operations, 2);
    assert_eq!(xy(&d, 0, 0), [(1., 1.), (2., 1.)]);
    assert_eq!(xy(&d, 0, 1), [(2., 2.), (3., 2.)]);
    assert_eq!(d.pages[0].operations[0].style.pen, 2);
    assert_eq!(d.pages[0].operations[1].style.pen, 3);
    assert_eq!(d.final_state.pen, 0);
    assert_eq!(read(b"SP2.5;").final_state.pen, 3);
}

#[test]
fn empty_moves_and_dots() {
    let d = read(b"SP1;PA;PR;PU;PD;PU;PD0,0;");
    assert_eq!(d.stats().operations, 2);
    assert_eq!(d.stats().vertices, 3);
    assert_eq!(xy(&d, 0, 0), [(0., 0.)]);
    assert_eq!(xy(&d, 0, 1), [(0., 0.), (0., 0.)]);
    assert!(d.to_svg(0, 0.).unwrap().contains("<circle"));
}

#[test]
fn bounds_exclude_travel_and_paper_size_is_unknown() {
    let d = read(b"SP1;PU-8000,4000;PU-80,-40;PD40,120;PU8000,8000;");
    let b = d.pages[0].bounds.unwrap();
    assert_eq!(b.min, Point { x: -2., y: -1. });
    assert_eq!(b.max, Point { x: 1., y: 3. });
    assert_eq!(d.pages[0].paper_size_mm, None);
    let empty = read(b"");
    assert_eq!(empty.pages.len(), 1);
    assert_eq!(empty.pages[0].bounds, None);
    assert!(empty.to_svg(0, 0.).unwrap().contains("width=\"1mm\""));
}

#[test]
fn pages_keep_mode_and_pen_but_reset_position_and_pen_down() {
    let d = read(b"BPINSP4PR40,0PD0,40PG;PG;PD40,0;PG;");
    assert_eq!(d.pages.len(), 2);
    assert_eq!(xy(&d, 0, 0), [(1., 0.), (1., 1.)]);
    assert_eq!(xy(&d, 1, 0), [(0., 0.), (1., 0.)]);
    assert_eq!(d.final_state.coordinate_mode, CoordinateMode::Relative);
    assert_eq!(d.final_state.pen, 4);
    assert!(!d.final_state.pen_down);
    assert_eq!(d.final_state.position, Point::default());
    let bp = read(b"SP4PR40,0PD0,40BP;PD40,0;");
    assert_eq!(bp.pages.len(), 2);
    assert_eq!(bp.final_state.coordinate_mode, CoordinateMode::Absolute);
    assert_eq!(xy(&bp, 1, 0), [(0., 0.), (1., 0.)]);
}

#[test]
fn forced_blank_pages_and_no_phantom_trailing_page() {
    assert_eq!(read(b"PG;PG;BP;IN;").pages.len(), 1);
    let d = read(b"PG1;PG0;PG-4;PG;IN;");
    assert_eq!(d.pages.len(), 3);
    assert!(d.pages.iter().all(|p| p.operations.is_empty()));
    assert_eq!(read(b"PG1;SP1PD;").pages.len(), 2);
}

#[test]
fn source_spans_and_optional_raw_retention() {
    let input = b" \nCO\"PD;SC\";DT#;INSP2PA0,0PD40,0;";
    let d = parse(
        input,
        InterpretOptions {
            preserve_commands: true,
            ..Default::default()
        },
    )
    .unwrap();
    let source = &d.pages[0].operations[0].source;
    assert_eq!(source.command_index, 5);
    assert_eq!(&input[source.span.start..source.span.end], b"PD40,0;");
    assert_eq!(d.raw.unwrap().source.as_ref(), input);
    assert!(read(input).raw.is_none());
}

#[test]
fn unsupported_commands_never_contaminate_later_geometry() {
    // Unknown mnemonics and BP with parameters are the remaining unsupported syntax;
    // device-control commands are accepted and geometry-affecting ones are ignored
    // with a warning instead (see device_control_commands_are_accepted_and_ignored_ones_are_reported).
    for instruction in ["ZZ;", "BP5;", "PS0,10;"] {
        let input = format!("SP1;PD40,0;{instruction}IN;SP2;PD80,0;");
        assert!(parse(input.as_bytes(), InterpretOptions::default()).is_err());
        let d = parse(input.as_bytes(), tolerant()).unwrap();
        assert_eq!(d.completeness, "partial");
        assert_eq!(d.stats().operations, 1, "{instruction}");
        assert_eq!(d.diagnostics.len(), 1);
        assert_eq!(d.diagnostics[0].action, "stop_interpretation");
        assert_eq!(d.final_state.position, Point { x: 1., y: 0. });
    }
}

#[test]
fn lexical_recovery_is_not_semantic_recovery() {
    let input = b"SP1;PD40,0;PR1e3,0;PD80,0;";
    let d = parse(input, tolerant()).unwrap();
    assert_eq!(d.raw.as_ref().unwrap().commands.len(), 3);
    assert_eq!(d.interpreted_commands, 2);
    assert_eq!(d.stats().operations, 1);
    assert_eq!(d.diagnostics[0].code, "EXPONENT_NOT_ALLOWED");
    assert_eq!(d.diagnostics[0].command.as_deref(), Some("PR"));
    let tail = parse(b"SP1;PD40,0;LBbad", tolerant()).unwrap();
    assert_eq!(tail.stats().operations, 1);
    assert_eq!(tail.completeness, "partial");
    // An earlier semantic error wins over a later lexical error.
    assert_eq!(
        parse(b"PS0;LBbad", InterpretOptions::default())
            .unwrap_err()
            .diagnostic
            .code,
        "INVALID_ARGUMENTS"
    );
}

#[test]
fn invalid_commands_are_atomic_and_range_is_bounded() {
    for (input, code) in [
        ("SP1;PD40,0,80;", "INVALID_ARGUMENTS"),
        ("SP1;SP2,3;", "INVALID_ARGUMENTS"),
        ("SP1;SP-1;", "INVALID_PEN"),
        ("SP1;PG1,2;", "INVALID_ARGUMENTS"),
        ("SP1;PGPD40,0;", "INVALID_TERMINATOR"),
        ("SP1;PD40,0,9000000,0;", "COORDINATE_RANGE"),
        ("SP1;PR8000000,0,8000000,0;", "COORDINATE_RANGE"),
    ] {
        let d = parse(input.as_bytes(), tolerant()).unwrap();
        assert_eq!(d.diagnostics[0].code, code, "{input}");
        assert_eq!(d.stats().operations, 0);
        assert_eq!(d.final_state.position, Point::default());
        assert!(!d.final_state.pen_down);
        assert_eq!(d.final_state.pen, 1);
        assert_eq!(d.final_state.coordinate_mode, CoordinateMode::Absolute);
    }
}

#[test]
fn geometry_limits_are_fatal_even_when_tolerant() {
    for (input, limits) in [
        (
            &b"SP1;PD40,0;PA80,0;"[..],
            GeometryLimits {
                max_operations: 1,
                ..Default::default()
            },
        ),
        (
            &b"SP1;PD40,0,80,0;"[..],
            GeometryLimits {
                max_vertices: 2,
                ..Default::default()
            },
        ),
        (
            &b"PG1;PG1;"[..],
            GeometryLimits {
                max_pages: 1,
                ..Default::default()
            },
        ),
        (
            &b""[..],
            GeometryLimits {
                max_pages: 0,
                ..Default::default()
            },
        ),
    ] {
        let e = parse(
            input,
            InterpretOptions {
                geometry_limits: limits,
                ..tolerant()
            },
        )
        .unwrap_err();
        assert_eq!(e.diagnostic.code, "LIMIT_EXCEEDED");
    }
    let d = parse(
        b"SP1;PD40,0;",
        InterpretOptions {
            geometry_limits: GeometryLimits {
                max_vertices: 2,
                max_operations: 1,
                max_pages: 1,
                ..Default::default()
            },
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(d.stats().vertices, 2);
}

#[test]
fn svg_preserves_continuity_but_never_connects_pen_up_travel() {
    let d = read(b"SP1;PU40,80;PD80,80;PR0,40;PU40,0;PD0,40;");
    let svg = d.to_svg(0, 1.).unwrap();
    assert_eq!(svg.matches("<path ").count(), 2);
    assert!(svg.contains("M1 2 L2 2 L2 3"));
    assert!(svg.contains("M3 3 L3 4"));
    assert!(svg.contains("scale(1,-1)"));
    assert!(svg.contains("data-completeness=\"complete\""));
    assert!(d.to_svg(1, 1.).is_err());
    for margin in [-1., f64::INFINITY, f64::NAN, 1e308] {
        assert!(d.to_svg(0, margin).is_err());
    }
}

#[test]
fn json_is_versioned_and_does_not_embed_raw_source() {
    let d = parse(b"SP1;PD40,80;AC0,0;", tolerant()).unwrap();
    let json: serde_json::Value = serde_json::from_str(&d.to_json().unwrap()).unwrap();
    assert_eq!(json["schema_version"], 1);
    assert_eq!(json["units"], "mm");
    assert_eq!(json["completeness"], "partial");
    assert!(json.get("raw").is_none());
    assert_eq!(
        json["pages"][0]["operations"][0]["geometry"]["points"][1]["y"],
        2.0
    );
    assert!(d
        .to_svg(0, 1.)
        .unwrap()
        .contains("data-completeness=\"partial\""));
}

#[test]
fn invalid_public_rust_geometry_cannot_emit_nan_or_xml_injection() {
    let mut d = read(b"SP1;PD40,0;");
    d.pages[0].operations[0].style.color = "\"/><g/>".into();
    assert!(d.to_svg(0, 1.).is_err());
    d.pages[0].operations[0].style.color = "#000000".into();
    let Geometry::Polyline { points, .. } = &mut d.pages[0].operations[0].geometry else {
        panic!()
    };
    points[0].x = f64::NAN;
    assert!(d.to_json().is_err());
    assert!(d.to_svg(0, 1.).is_err());
}
