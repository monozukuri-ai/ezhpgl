#![no_main]
use ezhpgl_core::{detect_format, parse_commands, Limits, ParseOptions};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = detect_format(data);
    for strict in [true, false] {
        if let Ok(doc) = parse_commands(
            data,
            ParseOptions {
                strict,
                limits: Limits {
                    max_input_bytes: 65_536,
                    max_command_bytes: 16_384,
                    max_commands: 1024,
                    max_arguments_per_command: 1024,
                    max_diagnostics: 32,
                },
            },
        ) {
            let mut end = 0;
            for c in &doc.commands {
                assert!(end <= c.span.start && c.span.start + 2 <= c.span.end);
                assert!(c.span.end <= data.len());
                assert!(c.payload_span.start <= c.payload_span.end);
                assert!(c.payload_span.end <= c.span.end);
                end = c.span.end;
            }
        }
    }
});
