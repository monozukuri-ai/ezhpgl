use ezhpgl_core::{detect_format, parse_commands, Limits, ParseOptions, Payload, RawDocument};

fn parse(data: &[u8]) -> RawDocument {
    parse_commands(data, ParseOptions::default()).unwrap()
}
fn tolerant(data: &[u8]) -> RawDocument {
    parse_commands(
        data,
        ParseOptions {
            strict: false,
            ..Default::default()
        },
    )
    .unwrap()
}
fn names(doc: &RawDocument) -> Vec<&str> {
    doc.commands.iter().map(|c| c.name()).collect()
}
fn error(data: &[u8]) -> &'static str {
    parse_commands(data, ParseOptions::default())
        .unwrap_err()
        .diagnostic
        .code
}

#[test]
fn concatenated_commands_and_lowercase_preserve_source_spans() {
    let data = b" \r\ninsp1pu100,100pd500,500;\n";
    let doc = parse(data);
    assert_eq!(names(&doc), ["IN", "SP", "PU", "PD"]);
    assert_eq!(doc.commands[0].span.start, 3);
    assert_eq!(doc.commands[0].span.end, 5);
    for command in &doc.commands {
        assert_eq!(
            data[command.span.start..command.span.start + 2].to_ascii_uppercase(),
            command.name
        );
        assert!(command.payload_span.start >= command.span.start + 2);
        assert!(command.payload_span.end <= command.span.end);
    }
}

#[test]
fn numeric_separators_and_lexemes() {
    let data = b"PA+10-20,.5,1.  -0.25;";
    let doc = parse(data);
    let Payload::Numbers(numbers) = &doc.commands[0].payload else {
        panic!()
    };
    assert_eq!(
        numbers.iter().map(|n| n.value).collect::<Vec<_>>(),
        [10., -20., 0.5, 1., -0.25]
    );
    assert_eq!(&data[numbers[2].span.start..numbers[2].span.end], b".5");
}

#[test]
fn numeric_arity_belongs_to_interpreter() {
    assert!(parse(b"PU1;").complete);
}

#[test]
fn exponent_is_not_confused_with_a_following_ea_command() {
    assert_eq!(error(b"PA1e3,2;"), "EXPONENT_NOT_ALLOWED");
    assert_eq!(error(b"PA1E-3,2;"), "EXPONENT_NOT_ALLOWED");
    assert_eq!(names(&parse(b"PA1,2EA3,4;")), ["PA", "EA"]);
}

#[test]
fn empty_parameters_and_malformed_decimals() {
    for data in [b"PA,1;".as_slice(), b"PA1,,2;", b"PA1,;", b"PA1,PU;"] {
        assert_eq!(error(data), "MISSING_PARAMETER");
    }
    for data in [b"PA.;".as_slice(), b"PA+;", b"PA1.2.3;"] {
        assert_eq!(error(data), "INVALID_NUMBER");
    }
    let huge = format!("PA{};", "9".repeat(400));
    assert_eq!(error(huge.as_bytes()), "INVALID_NUMBER");
}

#[test]
fn final_normal_command_needs_a_terminator() {
    assert_eq!(error(b"IN"), "MISSING_TERMINATOR");
    assert!(parse(b"IN\n").complete);
    assert!(parse(b"PA1 2 ").complete);
}

#[test]
fn labels_preserve_semicolons_commands_and_non_utf8() {
    let data = b"LB;PU;PD\xff\x80\x03SP2;";
    let doc = parse(data);
    assert_eq!(names(&doc), ["LB", "SP"]);
    let command = &doc.commands[0];
    assert_eq!(
        &data[command.payload_span.start..command.payload_span.end],
        b";PU;PD\xff\x80"
    );
    assert_eq!(
        command.payload,
        Payload::Label {
            terminator: 3,
            print_terminator: false
        }
    );
}

#[test]
fn custom_terminator_and_printing_mode() {
    let doc = parse(b"DT#,0;LBx;PD#;DT;LBy\x03");
    assert_eq!(names(&doc), ["DT", "LB", "DT", "LB"]);
    assert_eq!(
        doc.commands[1].payload,
        Payload::Label {
            terminator: b'#',
            print_terminator: true
        }
    );
    assert_eq!(
        doc.commands[3].payload,
        Payload::Label {
            terminator: 3,
            print_terminator: false
        }
    );
}

#[test]
fn dt_mode_separators_preserve_immediate_terminator() {
    for data in [b"DT# , 0;LBx#".as_slice(), b"DT# 0;LBx#", b"DT#, 0;LBx#"] {
        let doc = parse(data);
        assert_eq!(
            doc.commands[1].payload,
            Payload::Label {
                terminator: b'#',
                print_terminator: true
            }
        );
    }
    assert!(parse(b"DT# \n").complete);
}

#[test]
fn space_terminates_label_before_remaining_text() {
    let doc = tolerant(b"DT ;LBhello world\x03;");
    assert_eq!(names(&doc), ["DT", "LB", "WO"]);
    let span = doc.commands[1].payload_span;
    assert_eq!(&doc.source[span.start..span.end], b"hello");
    assert!(!doc.complete);
}

#[test]
fn immediate_space_is_not_trimmed_from_dt() {
    let doc = parse(b"DT ;LBhello SP1;");
    assert_eq!(names(&doc), ["DT", "LB", "SP"]);
    assert_eq!(
        doc.commands[1].payload,
        Payload::Label {
            terminator: b' ',
            print_terminator: false
        }
    );
    assert_eq!(names(&parse(b"DT7;LBitem7SP1;")), ["DT", "LB", "SP"]);
}

#[test]
fn resets_restore_label_framing() {
    for reset in ["IN", "DF", "BP"] {
        let data = format!("DT#;{reset};LB#PU;PD\x03SP1;");
        let doc = parse(data.as_bytes());
        assert_eq!(names(&doc), ["DT", reset, "LB", "SP"]);
        assert_eq!(
            doc.commands[2].payload,
            Payload::Label {
                terminator: 3,
                print_terminator: false
            }
        );
    }
    let doc = tolerant(b"DT#;IN1;LBx#;");
    assert_eq!(names(&doc), ["DT", "LB"]);
    assert_eq!(doc.diagnostics[0].code, "INVALID_RESET");
    assert_eq!(
        doc.commands[1].payload,
        Payload::Label {
            terminator: b'#',
            print_terminator: false
        }
    );
}

#[test]
fn begin_plot_without_parameters_can_precede_a_mnemonic() {
    assert_eq!(names(&parse(b"BPINPS16800,23760;")), ["BP", "IN", "PS"]);
    assert_eq!(error(b"BP1,\"title\";"), "UNSUPPORTED_SYNTAX");
    let doc = tolerant(b"BP1,\"title;IN\";SP1;");
    assert_eq!(names(&doc), ["BP", "SP"]);
    assert_eq!(doc.commands[0].payload, Payload::Opaque);
}

#[test]
fn invalid_dt_stops_before_uncertain_labels() {
    for data in [b"DT#,2;LBx#;IN;".as_slice(), b"DT\x00;SP1;", b"DT#,;LBx#;"] {
        assert_eq!(error(data), "INVALID_DT");
        let doc = tolerant(data);
        assert!(doc.commands.is_empty());
        assert_eq!(doc.diagnostics[0].action, "stop");
    }
}

#[test]
fn quoted_comment_is_not_command_text() {
    let data = b"CO \"LB;PE;DT#;\xff\"IN;";
    let doc = parse(data);
    assert_eq!(names(&doc), ["CO", "IN"]);
    let span = doc.commands[0].payload_span;
    assert_eq!(&data[span.start..span.end], b"LB;PE;DT#;\xff");
    assert_eq!(error(b"COplain;IN;"), "INVALID_COMMENT");
    assert_eq!(error(b"CO\"x\"1;"), "INVALID_DELIMITER");
}

#[test]
fn encoded_polyline_is_framed_without_decoding() {
    let data = b"PE:>=<7INLB\x80\xff;SP1;";
    let doc = parse(data);
    assert_eq!(names(&doc), ["PE", "SP"]);
    let span = doc.commands[0].payload_span;
    assert_eq!(&data[span.start..span.end], b":>=<7INLB\x80\xff");
    assert_eq!(doc.commands[0].payload, Payload::EncodedPolyline);
    assert!(parse(b"PE;").complete);
}

#[test]
fn missing_special_terminators_never_resynchronize() {
    for (data, code) in [
        (b"IN;LBhello;PU1,2;".as_slice(), "UNTERMINATED_LABEL"),
        (b"IN;PEabcdefSP1", "UNTERMINATED_PE"),
        (b"IN;CO\"PD;SP1;", "UNTERMINATED_COMMENT"),
    ] {
        assert_eq!(error(data), code);
        let doc = tolerant(data);
        assert_eq!(names(&doc), ["IN"]);
        assert!(!doc.complete);
        assert_eq!(doc.diagnostics[0].code, code);
        assert_eq!(doc.diagnostics[0].action, "stop");
    }
}

#[test]
fn unknown_grammar_is_explicit_and_opaque_in_tolerant_mode() {
    let data = b"IN;ZZ\"LB;PD\"abc;SP1;";
    assert_eq!(error(data), "UNSUPPORTED_SYNTAX");
    let doc = tolerant(data);
    assert_eq!(names(&doc), ["IN", "ZZ", "SP"]);
    assert_eq!(doc.commands[1].payload, Payload::Opaque);
    assert_eq!(doc.diagnostics[0].action, "preserve_opaque");
    assert!(!doc.complete);
    assert_eq!(names(&tolerant(b"ZZabcSP1")), Vec::<&str>::new());
}

#[test]
fn recover_only_across_numeric_tail_to_semicolon() {
    let doc = tolerant(b"IN;PA1e3,2;SP2;");
    assert_eq!(names(&doc), ["IN", "SP"]);
    assert_eq!(doc.diagnostics[0].span.start, 5);
    assert_eq!(doc.diagnostics[0].action, "skip_command");
    // A semicolon inside LB must never be mistaken for the invalid PA's end.
    let doc = tolerant(b"IN;PA1,LB;SP9;\x03SP2;");
    assert_eq!(names(&doc), ["IN"]);
    assert_eq!(doc.diagnostics[0].action, "stop");
}

#[test]
fn containers_and_escape_boundaries_are_not_stripped() {
    for data in [
        b"%PDF-1.7".as_slice(),
        b"%!PS",
        b"@PJL ENTER LANGUAGE=HPGL2",
        b"\x1b%-12345X",
        b"\x1b%0BIN;",
    ] {
        assert_eq!(error(data), "UNSUPPORTED_CONTAINER");
        assert!(parse_commands(
            data,
            ParseOptions {
                strict: false,
                ..Default::default()
            }
        )
        .is_err());
    }
    let doc = tolerant(b"IN;\x1b%0ASP1;");
    assert_eq!(names(&doc), ["IN"]);
    assert_eq!(doc.diagnostics[0].code, "UNSUPPORTED_CONTAINER");
}

#[test]
fn detection_is_prefix_based_and_dialect_ambiguous() {
    assert_eq!(detect_format(b" in;").language, "hpgl_family");
    assert_eq!(detect_format(b"PA0,0;").container, "plain");
    assert_eq!(detect_format(b"").language, "unknown");
    assert_eq!(detect_format(b"anything.plt").container, "unknown");
    assert_eq!(detect_format(b"\x1b%0B").container, "pcl_or_control");
    assert_eq!(detect_format(b"\x1b%-12345X").container, "pjl");
}

#[test]
fn empty_input_and_padding() {
    assert!(parse(b"").commands.is_empty());
    assert!(parse(b" \n;;\r\t").complete);
}

#[test]
fn resource_limits_are_fatal_even_when_tolerant() {
    let cases = [
        (
            b"IN;".as_slice(),
            Limits {
                max_input_bytes: 2,
                ..Default::default()
            },
        ),
        (
            b"LBabc\x03",
            Limits {
                max_command_bytes: 4,
                ..Default::default()
            },
        ),
        (
            b"IN;SP1;",
            Limits {
                max_commands: 1,
                ..Default::default()
            },
        ),
        (
            b"PA1,2;",
            Limits {
                max_arguments_per_command: 1,
                ..Default::default()
            },
        ),
        (
            b"ZZ;ZZ;",
            Limits {
                max_diagnostics: 1,
                ..Default::default()
            },
        ),
    ];
    for (data, limits) in cases {
        let result = parse_commands(
            data,
            ParseOptions {
                strict: false,
                limits,
            },
        );
        assert_eq!(result.unwrap_err().diagnostic.code, "LIMIT_EXCEEDED");
    }
    assert!(parse_commands(
        b"IN;",
        ParseOptions {
            strict: true,
            limits: Limits {
                max_command_bytes: 3,
                max_input_bytes: 3,
                max_commands: 1,
                ..Default::default()
            }
        }
    )
    .is_ok());
}

#[test]
fn deterministic_malformed_byte_stress_preserves_ranges_and_progress() {
    let mut seed = 0x19a0_22f3_u64;
    for length in 0..128 {
        for _ in 0..100 {
            let mut bytes = Vec::with_capacity(length + 2);
            bytes.extend_from_slice(b"PA");
            for _ in 0..length {
                seed ^= seed << 13;
                seed ^= seed >> 7;
                seed ^= seed << 17;
                bytes.push(seed as u8);
            }
            for strict in [true, false] {
                let result = parse_commands(
                    bytes.as_slice(),
                    ParseOptions {
                        strict,
                        limits: Limits {
                            max_commands: 16,
                            max_command_bytes: 256,
                            max_arguments_per_command: 16,
                            max_diagnostics: 8,
                            ..Default::default()
                        },
                    },
                );
                if let Ok(doc) = result {
                    let mut end = 0;
                    for c in &doc.commands {
                        assert!(end <= c.span.start);
                        assert!(c.span.start + 2 <= c.span.end);
                        assert!(c.span.end <= bytes.len());
                        assert!(c.payload_span.start <= c.payload_span.end);
                        assert!(c.payload_span.end <= c.span.end);
                        end = c.span.end;
                    }
                    assert_eq!(doc.complete, doc.diagnostics.is_empty());
                    for d in &doc.diagnostics {
                        assert!(d.span.start <= d.span.end && d.span.end <= bytes.len());
                    }
                }
            }
        }
    }
}
