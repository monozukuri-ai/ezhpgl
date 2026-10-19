use ezhpgl_core::{
    parse, parse_commands, Geometry, InterpretOptions, ParseOptions, PlotDocument, Point,
};

fn read(data: &[u8]) -> PlotDocument {
    parse(
        data,
        InterpretOptions {
            preserve_commands: true,
            ..Default::default()
        },
    )
    .unwrap()
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
const ENTER: &[u8] =
    b"\x1b%-12345X@PJL JOB NAME=\"test\"\r\n@PJL ENTER LANGUAGE=PCL\r\n\x1bE\x1b%0B";
fn mixed(body: &[u8], tail: &[u8]) -> Vec<u8> {
    [ENTER, body, tail].concat()
}

#[test]
fn wrapped_vectors_keep_full_input_spans_and_raw_contract() {
    let data = mixed(
        b"IN;SP1;PA40,80;PD120,160;",
        b"\x1b%0A\x0c\x1b%-12345X@PJL EOJ\r\n\x1b%-12345X",
    );
    let d = read(&data);
    assert_eq!(d.completeness, "complete");
    assert_eq!(d.pages.len(), 1);
    assert_eq!(d.stats().operations, 1);
    let op = &d.pages[0].operations[0];
    assert_eq!(
        &data[op.source.span.start..op.source.span.end],
        b"PD120,160;"
    );
    assert_eq!(op.source.command_index, 3);
    assert_eq!(d.source_size, data.len());
    assert_eq!(d.interpreted_commands, 4);
    assert_eq!(d.raw.as_ref().unwrap().source.as_ref(), data);
    assert!(d
        .input_events
        .iter()
        .all(|e| e.span.start < e.span.end && e.span.end <= data.len()));
    assert_eq!(
        parse_commands(&data, ParseOptions::default())
            .unwrap_err()
            .diagnostic
            .code,
        "UNSUPPORTED_CONTAINER"
    );
}

#[test]
fn direct_pjl_hpgl_and_pcl_hpgl_have_different_pg_behavior() {
    let body = b"SP1;PD40,0;PG;PD80,0;";
    let direct = read(&[b"@PJL ENTER LANGUAGE = HPGL2\n".as_slice(), body].concat());
    assert_eq!(direct.pages.len(), 2);
    let wrapped = read(&mixed(body, b""));
    assert_eq!(wrapped.pages.len(), 1);
    assert_eq!(
        wrapped.pages[0].operations[1].geometry.points()[0],
        Point { x: 1., y: 0. }
    );
    let plain = read(body);
    assert_eq!(plain.pages.len(), 2);
}

#[test]
fn mode_zero_roundtrip_keeps_cursor_attributes_and_dt() {
    let d = read(b"\x1b%0BSP2;SI.2,.3;DT#;LBA#\x1b%0A\x1b%0BLBB#PR40,0;PD40,0;");
    assert_eq!(d.stats().operations, 3);
    assert_eq!(d.pages[0].operations[2].style.pen, 2);
    let Geometry::Label { label } = &d.pages[0].operations[1].geometry else {
        panic!()
    };
    assert_eq!(label.start, Point { x: 3., y: 0. });
    assert_eq!(label.data, b"B");
    assert_eq!(d.final_state.position, Point { x: 8., y: 0. });
}

#[test]
fn escape_looking_label_comment_and_pe_payloads_are_opaque() {
    let d = read(b"\x1b%0BSP1;DT#;LB\x1b%0A\x1b%-12345X#CO\"\x1b%0A\";PD40,0;");
    assert_eq!(d.input_events.len(), 1);
    assert_eq!(d.stats().operations, 2);
    assert_eq!(d.interpreted_commands, 5);
    let Geometry::Label { label } = &d.pages[0].operations[0].geometry else {
        panic!()
    };
    assert!(label.data.contains(&27));
    let d = parse(b"\x1b%0BSP1;PE\x1b%0A;PD40,0;", tolerant()).unwrap();
    assert_eq!(d.input_events.len(), 1);
    assert_eq!(d.interpreted_commands, 1);
    assert_eq!(d.stats().operations, 0);
}

#[test]
fn binary_raster_lengths_are_not_scanned_for_commands_or_page_breaks() {
    let payload = b"\x1b%0BSP9;PD999,999;\x1b%-12345X\x0cLBbad\0";
    let mut data = mixed(b"SP1;PD40,0;", b"\x1b%0A\x1b*r1A\x1b*t300R\x1b*b0M");
    data.extend_from_slice(format!("\x1b*b{}W", payload.len()).as_bytes());
    data.extend_from_slice(payload);
    data.extend_from_slice(b"\x1b*rB\x1b%0BPD80,0;");
    let d = read(&data);
    assert_eq!(d.pages.len(), 1);
    assert_eq!(d.stats().operations, 2);
    assert_eq!(d.final_state.pen, 1);
    assert_eq!(d.final_state.position, Point { x: 2., y: 0. });
    assert!(d.pages[0].omitted_content);
    assert_eq!(d.diagnostics.len(), 1);
    assert_eq!(d.diagnostics[0].code, "OMITTED_RASTER");
    assert_eq!(d.diagnostics[0].action, "omit_content");
    let e = d
        .input_events
        .iter()
        .find(|e| e.kind == "omitted_raster_data")
        .unwrap();
    let p = e.payload_span.unwrap();
    assert_eq!(&data[p.start..p.end], payload);
    assert!(d
        .to_svg(0, 1.)
        .unwrap()
        .contains("data-omitted-content=\"true\""));
}

#[test]
fn pcl_text_and_raster_only_pages_are_retained_without_fabricated_bounds() {
    let d = read(b"\x1bEhello\x0c\x1b*b3Wabc\x0c\x1b%0BSP1;PD40,0;");
    assert_eq!(d.pages.len(), 3);
    assert!(d.pages[0].omitted_content && d.pages[1].omitted_content);
    assert!(!d.pages[2].omitted_content);
    assert!(d.pages[0].bounds.is_none());
    assert_eq!(d.diagnostics.len(), 2);
    assert_eq!(d.stats().operations, 1);
    let d = read(b"\x1bE \t\r\n\x1b%0BSP1;PD40,0;");
    assert_eq!(d.completeness, "complete");
}

#[test]
fn reset_eoj_and_ff_page_lifecycle_has_no_phantom_tail() {
    let d=read(b"\x1b%0BSP1;DT#;PD40,0;\x1b%0A\x0c\x1b%0BLBA#\x1bE\x1b%0BSP2;LBB\x03\x1b%-12345X@PJL EOJ\n");
    assert_eq!(d.pages.len(), 3);
    assert_eq!(d.pages[1].operations[0].style.pen, 1);
    assert_eq!(d.pages[2].operations[0].style.pen, 2);
    assert!(d.final_state.text.is_none());
    assert_eq!(d.final_state.position, Point::default());
    let d = read(b"\x1bE\x0c\x0c");
    assert_eq!(d.pages.len(), 2);
    let d = read(b"\x1bE\x1bE");
    assert_eq!(d.pages.len(), 1);
}

#[test]
fn uel_alone_does_not_reset_hpgl_or_eject_a_page() {
    let d=read(b"\x1b%0BSP1;PD40,0;\x1b%-12345X@PJL COMMENT separator\n@PJL ENTER LANGUAGE=PCL\n\x1b%0BPD80,0;");
    assert_eq!(d.pages.len(), 1);
    assert_eq!(
        d.pages[0].operations[1].geometry.points()[0],
        Point { x: 1., y: 0. }
    );
}

#[test]
fn unsupported_binary_settings_and_cursor_transfer_stop_at_valid_prefix() {
    for tail in [
        b"\x1b%1A".as_slice(),
        b"\x1b%2A",
        b"\x1b%-1B",
        b"\x1b%0A\x1b&l2A",
        b"\x1b%0A\x1b*b1m2Wab",
        b"\x1b%0A\x1b*v3Wabc",
        b"\x1b%0A\x1b(2X",
        b"\x1b%0A\x1b*r-1A",
    ] {
        let data = mixed(b"SP1;PD40,0;", &[tail, b"\x1b%0BPD800,0;"].concat());
        assert!(parse(&data, InterpretOptions::default()).is_err());
        let d = parse(&data, tolerant()).unwrap();
        assert_eq!(d.stats().operations, 1);
        assert_eq!(d.final_state.position, Point { x: 1., y: 0. });
        assert_eq!(d.diagnostics.last().unwrap().action, "stop_interpretation");
    }
}

#[test]
fn truncated_binary_and_pjl_never_recover_into_later_hpgl() {
    for (tail, code) in [
        (
            b"\x1b%0A\x1b*b99W\x1b%0BPD800,0;".as_slice(),
            "TRUNCATED_BINARY",
        ),
        (
            b"\x1b%-12345X@PJL ENTER LANGUAGE=PCL".as_slice(),
            "UNTERMINATED_PJL",
        ),
        (b"\x1b%".as_slice(), "TRUNCATED_ESCAPE"),
    ] {
        let d = parse(&mixed(b"SP1;PD40,0;", tail), tolerant()).unwrap();
        assert_eq!(d.stats().operations, 1);
        assert_eq!(d.diagnostics[0].code, code);
    }
}

#[test]
fn unknown_pjl_and_other_languages_fail_without_searching_body() {
    for line in [
        "@PJL SET COPIES=2",
        "@PJL JOB START=2 END=3",
        "@PJL EOJ NAME=\"test\" UNKNOWN=1",
        "@PJL ENTER LANGUAGE=POSTSCRIPT",
        "@PJL ENTER LANGUAGE=HPGL2 junk",
        "garbage",
        "@PJL ENTER LANGUAGE=\"PCL\"",
    ] {
        let data = format!("\x1b%-12345X{line}\n\x1b%0BSP1;PD40,0;");
        let d = parse(data.as_bytes(), tolerant()).unwrap();
        assert_eq!(d.stats().operations, 0);
        assert_eq!(d.diagnostics[0].code, "UNSUPPORTED_PJL");
    }
}

#[test]
fn container_records_and_payloads_obey_shared_limits() {
    for (data, kind) in [
        (b"\x1b%0BSP1;PD40,0;".as_slice(), 0),
        (b"\x1bE\x1b*b9W123456789".as_slice(), 1),
        (b"@PJL COMMENT abcdefghijklmnop\n".as_slice(), 1),
    ] {
        let mut o = tolerant();
        if kind == 0 {
            o.parse.limits.max_commands = 2;
        } else {
            o.parse.limits.max_command_bytes = 12;
        }
        assert_eq!(
            parse(data, o).unwrap_err().diagnostic.code,
            "LIMIT_EXCEEDED"
        );
    }
    let mut o = tolerant();
    o.geometry_limits.max_pages = 1;
    assert_eq!(
        parse(b"\x1bE\x0c\x0c", o).unwrap_err().diagnostic.code,
        "LIMIT_EXCEEDED"
    );
}
