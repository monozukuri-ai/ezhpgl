from __future__ import annotations

import gc
import json
import subprocess
import sys
import xml.etree.ElementTree as ET
from pathlib import Path

import pytest

import ezhpgl

SVG = "{http://www.w3.org/2000/svg}"
GOLDEN = Path(__file__).parents[1] / "golden"


def test_plot_golden_and_stats():
    doc = ezhpgl.readfile(GOLDEN / "plot.hpgl", preserve_commands=True)
    assert isinstance(doc, ezhpgl.PlotDocument)
    assert json.loads(doc.to_json()) == json.loads((GOLDEN / "plot.json").read_text())
    assert (doc.profile, doc.units, doc.schema_version) == ("generic-plotter", "mm", 1)
    assert doc.stats() == {
        "pages": 2, "operations": 3, "vertices": 5, "interpreted_commands": 7,
        "source_bytes": 37,
    }
    assert doc.raw.completeness == "complete"
    assert len(doc.raw) == 7
    assert ezhpgl.parse(b"IN;").raw is None


def test_shared_geometry_survives_document_and_input_deletion():
    data = b"SP2;PA40,80;PD80,80;SP3;PR0,40;"
    doc = ezhpgl.parse(data)
    page = doc.pages[0]
    first, second = page.operations
    geometry = first.geometry
    del data, doc, page
    gc.collect()
    assert geometry.kind == "polyline"
    assert [(p.x, p.y) for p in geometry.points] == [(1, 2), (2, 2)]
    assert (first.style.pen, second.style.pen) == (2, 3)
    assert first.style.width_mm == 0.35
    assert first.style.color == "#000000"
    assert first.source.command == "PD"
    assert first.source.command_index == 2
    assert geometry.start_dot is True
    assert second.geometry.start_dot is False
    for obj, name, value in [(geometry.points[0], "x", 10), (first.style, "pen", 4),
                             (first.source.span, "start", 0), (first, "path_id", 0)]:
        with pytest.raises(AttributeError):
            setattr(obj, name, value)


def test_svg_units_orientation_continuity_and_dot():
    doc = ezhpgl.readfile(GOLDEN / "plot.hpgl")
    root = ET.fromstring(doc.to_svg())
    assert root.attrib["data-completeness"] == "complete"
    assert root.attrib["width"].endswith("mm")
    group = root.find(f"{SVG}g")
    assert group.attrib["transform"] == "scale(1,-1)"
    paths = group.findall(f"{SVG}path")
    assert len(paths) == 1
    assert paths[0].attrib["d"] == "M1 2 L2 2 L2 3"
    assert paths[0].attrib["data-pen"] == "2"
    point_root = ET.fromstring(doc.to_svg(page=1, margin_mm=0))
    assert point_root.find(f"{SVG}g/{SVG}circle").attrib["r"] == "0.175"
    assert all(float(v) > 0 for v in point_root.attrib["viewBox"].split()[2:])
    assert doc.pages[0].paper_size_mm is None
    assert doc.pages[0].bounds.min.x == 1
    assert doc.pages[0].bounds.max.y == 3


def test_write_outputs_pathlike_and_no_writes_on_invalid_export(tmp_path):
    doc = ezhpgl.readfile(GOLDEN / "plot.hpgl")
    output = tmp_path / "図面.svg"
    assert doc.write_svg(output, page=1) is None
    assert output.read_text() == doc.to_svg(page=1)
    target = tmp_path / "drawing.json"
    assert doc.write_json(str(target)) is None
    assert json.loads(target.read_text()) == json.loads(doc.to_json())
    with pytest.raises(ValueError, match="Page index"):
        doc.write_svg(output, page=2)
    assert output.read_text() == doc.to_svg(page=1)
    with pytest.raises(OSError):
        doc.write_json(tmp_path / "missing" / "drawing.json")


@pytest.mark.parametrize("data,code", [
    (b"PS0;PD80,0;", "INVALID_ARGUMENTS"),
    (b"BP5;PD80,0;", "UNSUPPORTED_SYNTAX"),
    (b"ZZ;PD80,0;", "UNSUPPORTED_SYNTAX"),
    (b"PR1e2,0;PD80,0;", "EXPONENT_NOT_ALLOWED"),
    (b"PD40,40,80;", "INVALID_ARGUMENTS"),
])
def test_partial_geometry_stops_at_first_uncertainty(data, code):
    prefix = b"SP1;PD40,0;"
    with pytest.raises(ezhpgl.ParseError) as error:
        ezhpgl.parse(prefix + data)
    assert error.value.diagnostic.code == code
    doc = ezhpgl.parse(prefix + data, strict=False, preserve_commands=True)
    assert doc.completeness == "partial"
    assert doc.stats()["operations"] == 1
    assert doc.interpreted_commands == 2
    assert doc.diagnostics[0].action == "stop_interpretation"
    assert doc.final_state.position.x == 1
    assert doc.final_state.pen_down
    assert ET.fromstring(doc.to_svg()).attrib["data-completeness"] == "partial"


def test_raw_and_plot_completeness_have_different_scopes():
    data = b"IN;SP1;PEAB;"
    assert ezhpgl.raw.parse_commands(data).completeness == "complete"
    doc = ezhpgl.parse(data, strict=False, preserve_commands=True)
    assert doc.raw.completeness == "complete"
    assert doc.completeness == "partial"


@pytest.mark.parametrize("limits", [
    ezhpgl.GeometryLimits(max_pages=0), ezhpgl.GeometryLimits(max_operations=0),
    ezhpgl.GeometryLimits(max_vertices=1),
])
def test_geometry_limits_are_fatal(limits):
    with pytest.raises(ezhpgl.ParseError) as error:
        ezhpgl.parse(b"SP1;PD40,0;", strict=False, geometry_limits=limits)
    assert error.value.diagnostic.code == "LIMIT_EXCEEDED"


def test_bounded_readfile_and_input_errors(tmp_path):
    path = tmp_path / "large.hpgl"
    path.write_bytes(b"IN;" * 100)
    with pytest.raises(ezhpgl.ParseError) as error:
        ezhpgl.readfile(path, strict=False, limits=ezhpgl.Limits(max_input_bytes=8))
    assert error.value.diagnostic.code == "LIMIT_EXCEEDED"
    assert error.value.diagnostic.span.end == 9
    with pytest.raises(OSError):
        ezhpgl.readfile(tmp_path / "absent")
    with pytest.raises(TypeError):
        ezhpgl.parse("IN;")
    for margin in [-1, float("nan"), float("inf")]:
        with pytest.raises(ValueError):
            ezhpgl.parse(b"").to_svg(margin_mm=margin)


@pytest.mark.parametrize("format,page,expected", [("json", None, 0), ("svg", 2, 0), ("svg", 3, 1)])
def test_installed_export_cli_from_other_directory(tmp_path, format, page, expected):
    output = tmp_path / f"out.{format}"
    command = [sys.executable, "-m", "ezhpgl", "export", str(GOLDEN / "plot.hpgl"),
               "--format", format, "-o", str(output)]
    if page is not None:
        command.extend(["--page", str(page)])
    result = subprocess.run(command, cwd=tmp_path, capture_output=True, text=True)
    assert result.returncode == expected, result.stderr
    if expected == 0:
        assert json.loads(result.stdout)["completeness"] == "complete"
        if format == "json":
            assert json.loads(output.read_text())["pages"][1]["index"] == 1
        else:
            assert ET.fromstring(output.read_text()).find(f"{SVG}title").text.startswith("HP-GL page 2")
    else:
        assert not output.exists()


@pytest.mark.parametrize("tolerant,expected", [(False, 1), (True, 2)])
def test_cli_partial_output_is_explicit(tmp_path, tolerant, expected):
    source = tmp_path / "partial.hpgl"
    source.write_bytes(b"SP1;PD40,0;BP5;PD400,0;")
    output = tmp_path / "partial.svg"
    command = [sys.executable, "-m", "ezhpgl", "export", str(source), "--format", "svg", "-o", str(output)]
    if tolerant:
        command.append("--tolerant")
    result = subprocess.run(command, capture_output=True, text=True)
    assert result.returncode == expected
    if tolerant:
        assert json.loads(result.stdout)["diagnostics"][0]["command"] == "BP"
        assert ET.fromstring(output.read_text()).attrib["data-completeness"] == "partial"
    else:
        assert not output.exists()


def test_large_drawing_remains_bounded_and_is_not_expanded_to_dicts():
    data = b"SP1;PD;PR;" + b"PA0,0,40,40;" * 10000
    doc = ezhpgl.parse(data)
    assert doc.stats()["operations"] == 10001
    assert doc.stats()["vertices"] == 30001
    assert doc.raw is None
    assert doc.final_state.position.x == 1
