from __future__ import annotations

import gc
import json
import subprocess
import sys
import xml.etree.ElementTree as ET
from pathlib import Path

import ezhpgl
import pytest


def test_label_glyph_state_and_source_outlive_input_document():
    data = b"SP1;SI.2,.3;SD1,14;PA40,80;LBcaf\xe9\x03PD400,80;"
    doc = ezhpgl.parse(data, preserve_commands=True)
    op = doc.pages[0].operations[0]
    geometry, state = op.geometry, doc.final_state
    label = geometry.label
    del doc, data
    gc.collect()
    assert isinstance(label, ezhpgl.Label)
    assert geometry.kind == "label" and geometry.points == []
    assert geometry.curve is None and geometry.contours == [] and geometry.fill is None
    assert label.data == b"caf\xe9" and label.decoded == "café"
    assert label.terminator == 3 and not label.print_terminator and label.approximate
    assert label.unmapped_offsets == [] and op.approximate
    assert label.glyphs[3].character == "é"
    assert label.glyphs[3].source_offset == label.payload_span.start + 3
    assert (label.glyphs[3].origin.x, label.glyphs[3].origin.y) == (10, 2)
    assert (label.start.x, label.end.x) == (1, 13)
    assert label.glyphs[0].font.symbol_set == 14
    assert label.state.size_mode == "absolute"
    assert state.text.standard.symbol_set == 14
    assert state.text.carriage_return.x == 10 and not state.position_approximate
    for obj, name in [(label, "decoded"), (label.glyphs[0], "character"),
                      (label.glyphs[0].font, "typeface"), (label.state, "width"),
                      (op, "approximate"), (state, "position_approximate")]:
        with pytest.raises(AttributeError):
            setattr(obj, name, None)
    glyphs = label.glyphs
    glyphs.clear()
    assert len(label.glyphs) == 4


def test_approximation_is_nonfatal_in_strict_mode_and_export_metadata_matches():
    doc = ezhpgl.parse(b"SP1;SI.2,.3;LB<&\x03PR40,0;PD40,0;PU0,0;PD40,0;")
    assert doc.completeness == "partial"
    assert len(doc.diagnostics) == 1 and doc.diagnostics[0].action == "approximate_text"
    assert doc.interpreted_commands == 7
    assert doc.pages[0].operations[1].approximate
    # PR mode is still active: relative PU does not remove text dependence.
    assert doc.final_state.position_approximate
    data = json.loads(doc.to_json())
    item = data["pages"][0]["operations"][0]
    assert item["approximate"] and item["geometry"]["kind"] == "label"
    assert item["geometry"]["label"]["data"] == [60, 38]
    root = ET.fromstring(doc.to_svg())
    assert root.attrib["data-completeness"] == "partial"
    assert [x.text for x in root.findall(".//{http://www.w3.org/2000/svg}text")] == ["<", "&"]


def test_transparent_label_preserves_binary_and_printed_terminator():
    doc = ezhpgl.parse(b"SP1;TD1;DT#,0;LB\0\n\xff#")
    label = doc.pages[0].operations[0].geometry.label
    assert label.data == b"\0\n\xff" and label.decoded == "�� #"
    assert len(label.glyphs) == 4 and label.print_terminator
    assert label.unmapped_offsets == [label.payload_span.start, label.payload_span.start + 1]
    assert label.glyphs[-1].source_offset == label.payload_span.end


@pytest.mark.parametrize("data,limits", [
    (b"SP1;LBA\x03", ezhpgl.GeometryLimits(max_glyphs=0)),
    (b"SP0;LBA\x03LBB\x03", ezhpgl.GeometryLimits(max_glyphs=1)),
    (b"SP1;LBab\x03", ezhpgl.GeometryLimits(max_label_bytes=1)),
    (b"SP1;LBA\x03", ezhpgl.GeometryLimits(max_vertices=3)),
])
def test_text_limits_are_bounded_even_for_invisible_text(data, limits):
    with pytest.raises(ezhpgl.ParseError) as exc:
        ezhpgl.parse(data, strict=False, geometry_limits=limits)
    assert exc.value.diagnostic.code == "LIMIT_EXCEEDED"
    with pytest.raises(AttributeError):
        limits.max_glyphs = 1
    assert ezhpgl.GeometryLimits().max_label_bytes == 65536


def test_container_event_spans_binary_payload_and_page_omissions():
    payload = b"\x1b%0BSP9;PD999,999;\x0c"
    data = (b"\x1bE\x1b%0BSP1;PD40,0;\x1b%0A\x1b*b" + str(len(payload)).encode()
            + b"W" + payload + b"\x0c\x1b%0BPD80,0;")
    doc = ezhpgl.parse(data, preserve_commands=True)
    event = next(e for e in doc.input_events if e.kind == "omitted_raster_data")
    assert isinstance(event, ezhpgl.InputEvent)
    assert data[event.payload_span.start:event.payload_span.end] == payload
    assert doc.source_size == len(data)
    assert len(doc.pages) == 2 and doc.pages[0].omitted_content
    assert not doc.pages[1].omitted_content and doc.final_state.pen == 1
    assert doc.stats()["operations"] == 2 and len(doc.raw.commands) == 3
    assert doc.diagnostics[0].code == "OMITTED_RASTER"
    assert json.loads(doc.to_json())["input_events"][0]["kind"] == "reset"
    with pytest.raises(AttributeError):
        event.kind = "fake"
    del doc
    gc.collect()
    assert event.command == "*bW"
    with pytest.raises(ezhpgl.ParseError):
        ezhpgl.raw.parse_commands(data)


@pytest.mark.parametrize("suffix,code", [
    (b"\x1b%1A", "UNSUPPORTED_MODE_SWITCH"),
    (b"\x1b%0A\x1b&l2A", "UNSUPPORTED_PCL"),
    (b"\x1b%0A\x1b*b100Wbad", "TRUNCATED_BINARY"),
    (b"\x1b%-12345X@PJL ENTER LANGUAGE=PCL", "UNTERMINATED_PJL"),
])
def test_unsupported_containers_keep_only_valid_prefix(suffix, code):
    data = b"\x1b%0BSP1;PD40,0;" + suffix
    doc = ezhpgl.parse(data, strict=False)
    assert doc.stats()["operations"] == 1
    assert doc.diagnostics[-1].code == code
    assert doc.final_state.position.x == 1
    with pytest.raises(ezhpgl.ParseError):
        ezhpgl.parse(data)


def test_cli_exports_approximate_labels_and_mixed_second_page(tmp_path):
    root = Path(__file__).parents[2]
    for name, page in [("labels.hpgl", 1), ("mixed-pages.pcl", 2)]:
        path = root / "examples" / name
        output = tmp_path / f"{name}.svg"
        result = subprocess.run([sys.executable, "-m", "ezhpgl", "export", str(path),
                                 "--format", "svg", "--page", str(page), "-o", str(output)],
                                capture_output=True, text=True, check=False)
        assert result.returncode == 2, result.stderr
        assert json.loads(result.stdout)["completeness"] == "partial"
        assert ET.parse(output).getroot().findall(".//{http://www.w3.org/2000/svg}text")
        doc = ezhpgl.readfile(path)
        assert doc.interpreted_commands > 0
        assert all(d.action != "stop_interpretation" for d in doc.diagnostics)


def test_plain_geometry_keeps_m2_json_shape():
    doc = ezhpgl.parse(b"SP1;PD40,0;")
    assert doc.input_events == [] and doc.final_state.text is None
    assert doc.pages[0].operations[0].geometry.label is None
    data = json.loads(doc.to_json())
    assert "input_events" not in data and "text" not in data["final_state"]
    assert "approximate" not in data["pages"][0]["operations"][0]
