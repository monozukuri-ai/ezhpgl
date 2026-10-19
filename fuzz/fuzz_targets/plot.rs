#![no_main]
use ezhpgl_core::{parse, DeviceProfile, GeometryLimits, InterpretOptions, Limits, ParseOptions};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    for strict in [true, false] {
        let options = InterpretOptions {
            parse: ParseOptions {
                strict,
                limits: Limits {
                    max_input_bytes: 65_536,
                    max_command_bytes: 16_384,
                    max_commands: 256,
                    max_arguments_per_command: 1024,
                    max_diagnostics: 32,
                },
            },
            preserve_commands: true,
            profile: DeviceProfile {
                plot_size_mm: Some([200., 100.]),
                paper_size_mm: None,
            },
            geometry_limits: GeometryLimits {
                max_pages: 32,
                max_operations: 128,
                max_vertices: 1024,
                max_curve_segments: 256,
                max_polygon_vertices: 1024,
                max_polygon_contours: 32,
                max_hatch_lines: 128,
                max_label_bytes: 2048,
                max_glyphs: 256,
            },
        };
        if let Ok(doc) = parse(data, options) {
            let stats = doc.stats();
            assert!(stats.pages > 0 && stats.pages <= 32);
            assert!(stats.operations <= 128 && stats.vertices <= 1024);
            assert_eq!(doc.completeness == "complete", doc.diagnostics.is_empty());
            let raw = doc.raw.as_ref().unwrap();
            for event in &doc.input_events {
                assert!(event.span.start < event.span.end && event.span.end <= data.len());
                if let Some(p) = event.payload_span {
                    assert!(
                        p.start >= event.span.start && p.start <= p.end && p.end <= event.span.end
                    );
                }
            }
            for (index, page) in doc.pages.iter().enumerate() {
                assert_eq!(page.index, index);
                for op in &page.operations {
                    assert_eq!(op.source.span, raw.commands[op.source.command_index].span);
                    assert!(op.source.command_index < doc.interpreted_commands);
                    assert!(
                        op.geometry.vertex_count() > 0
                            || matches!(op.geometry, ezhpgl_core::Geometry::Label { .. })
                    );
                    if let ezhpgl_core::Geometry::Label { label } = &op.geometry {
                        assert_eq!(
                            &data[label.payload_span.start..label.payload_span.end],
                            label.data
                        );
                        assert!(label.glyphs.iter().all(|g| g.source_offset < data.len()));
                    }
                }
                assert!(matches!(
                    doc.to_svg(index, 1.0),
                    Ok(_) | Err(ezhpgl_core::ExportError::RenderLimit)
                ));
            }
            assert!(doc.to_json().is_ok());
        }
    }
});
