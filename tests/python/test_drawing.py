from __future__ import annotations

import gc
import json
import xml.etree.ElementTree as ET
from pathlib import Path

import ezhpgl
import pytest


def test_analytic_curves_and_contours_outlive_document():
    data = b"SP1;CI80,90;PD;BZ0,40,40,40,40,0;PM0;CI40,90;PM2;FP;"
    doc = ezhpgl.parse(data, preserve_commands=True)
    circle, _, bezier, polygon = [op.geometry for op in doc.pages[0].operations]
    contour = polygon.contours[1]
    del data, doc, polygon
    gc.collect()
    assert circle.kind == "curve"
    assert isinstance(circle.curve, ezhpgl.Curve)
    assert circle.curve.kind == "elliptic_arc"
    assert (circle.curve.center.x, circle.curve.axis_u.x, circle.curve.axis_v.y) == (0, 2, 2)
    assert (circle.curve.chord_mode, circle.curve.chord_value) == (0, 90)
    assert len(circle.points) == 5
    assert circle.contours == [] and circle.fill is None
    assert bezier.curve.kind == "cubic_bezier"
    assert [(p.x, p.y) for p in bezier.curve.control_points] == [(0, 0), (0, 1), (1, 1), (1, 0)]
    assert bezier.curve.tolerance_mm == 0.01
    assert isinstance(contour, ezhpgl.Contour)
    assert contour.edges[0].source.command == "CI"
    assert contour.edges[0].curve.kind == "elliptic_arc"
    assert contour.edges[0].pen_down
    assert (contour.edges[0].start, contour.edges[0].end) == (0, 4)
    for obj, attr, value in [(circle.curve, "kind", "x"), (contour, "points", []),
                             (contour.edges[0], "pen_down", False)]:
        with pytest.raises(AttributeError):
            setattr(obj, attr, value)
    points = contour.points
    points.clear()
    assert len(contour.points) == 5


def test_style_fill_hatches_json_and_svg_are_consistent():
    doc = ezhpgl.parse(b"NP4;PC2,12,34,56;SP2;PW.5;LT2,4,1;LA1,4,2,5;FT3,40;RA400,400;")
    op = doc.pages[0].operations[0]
    assert (op.style.color, op.style.width_mm) == ("#0c2238", 0.5)
    assert isinstance(op.style.line, ezhpgl.LineStyle)
    assert (op.style.line.cap, op.style.line.join) == (4, 5)
    assert op.style.line.pattern_length_mm == 4
    g = op.geometry
    assert g.kind == "polygon" and g.points == [] and g.curve is None
    assert (g.fill.fill_type, g.fill.rule, g.fill.spacing_mm) == (3, "evenodd", 1)
    assert len(g.fill.hatch_lines) == 10
    assert g.fill.hatch_phases_mm[:3] == [0, 2, 0]
    assert not doc.final_state.polygon_mode
    parsed = json.loads(doc.to_json())["pages"][0]["operations"][0]
    assert parsed["geometry"]["fill"]["hatch_phases_mm"] == g.fill.hatch_phases_mm
    assert parsed["geometry"]["contours"][0]["edges"][0]["source"]["command"] == "RA"
    paths = ET.fromstring(doc.to_svg()).findall(".//{http://www.w3.org/2000/svg}path")
    assert len(paths) == 10
    assert all(p.attrib["stroke"] == op.style.color for p in paths)
    assert paths[1].attrib["stroke-dashoffset"] == "-2"
    with pytest.raises(AttributeError):
        g.fill.rule = "nonzero"


@pytest.mark.parametrize("limits,data", [
    (ezhpgl.GeometryLimits(max_curve_segments=4), b"CI40;"),
    (ezhpgl.GeometryLimits(max_polygon_vertices=2), b"PM0;PD40,0,40,40;PM2;"),
    (ezhpgl.GeometryLimits(max_polygon_contours=0), b"PM0;"),
    (ezhpgl.GeometryLimits(max_hatch_lines=1), b"SP1;FT4,40;RA400,400;"),
])
def test_new_limits_remain_fatal_and_readonly(limits, data):
    with pytest.raises(ezhpgl.ParseError) as exc:
        ezhpgl.parse(data, strict=False, geometry_limits=limits)
    assert exc.value.diagnostic.code == "LIMIT_EXCEEDED"
    with pytest.raises(AttributeError):
        limits.max_curve_segments = 10


@pytest.mark.parametrize("data,code", [
    (b"PM0;PD40,0;", "UNCLOSED_POLYGON"),
    (b"FT10,50;", "UNSUPPORTED_ATTRIBUTE"),
    (b"LA1,3;", "UNSUPPORTED_ATTRIBUTE"),
    (b"PD;AT80,0,40,0;", "UNSUPPORTED_GEOMETRY"),
])
def test_partial_profile_boundaries_remain_explicit(data, code):
    doc = ezhpgl.parse(data, strict=False)
    assert doc.completeness == "partial"
    assert doc.diagnostics[0].code == code
    with pytest.raises(ezhpgl.ParseError):
        ezhpgl.parse(data)


def test_svg_work_limit_does_not_overwrite_output(tmp_path):
    doc = ezhpgl.parse(b"SP1;LT1,.000001,1;PD4000,0;")
    output = tmp_path / "existing.svg"
    output.write_text("keep")
    with pytest.raises(ValueError):
        doc.write_svg(output)
    assert output.read_text() == "keep"


def test_example_is_complete_and_all_shapes_have_source_spans():
    path = Path(__file__).parents[2] / "examples" / "curves-and-fills.hpgl"
    data = path.read_bytes()
    doc = ezhpgl.parse(data, preserve_commands=True)
    assert doc.completeness == "complete"
    assert len(doc.pages) == 2
    for page in doc.pages:
        for op in page.operations:
            assert data[op.source.span.start:op.source.span.start + 2].decode() == op.source.command
        ET.fromstring(doc.to_svg(page=page.index))
