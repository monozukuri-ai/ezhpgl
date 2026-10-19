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


def test_profile_transform_clip_and_owned_wrappers():
    profile = ezhpgl.DeviceProfile(plot_size_mm=(100, 50), paper_size_mm=(110, 60))
    doc = ezhpgl.parse(b"RO90;IP;SC0,10,0,10;IW2,2,8,8;SP1;PU0,5;PD10,5;",
                       profile=profile, preserve_commands=True)
    page = doc.pages[0]
    op = page.operations[0]
    state = doc.final_state
    assert page.plot_size_mm == (100, 50)
    assert page.paper_size_mm == (110, 60)
    assert doc.device_profile.plot_size_mm == (100, 50)
    assert doc.device_profile.paper_size_mm == (110, 60)
    assert (state.position.x, state.position.y) == (50, 50)
    assert state.transform.rotation == 90
    assert state.transform.scaling == "anisotropic"
    assert list(state.transform.matrix) == [0, 5, -10, 0, 100, 0]
    assert state.transform.p2_plotter_units.y == 4000
    assert state.transform.p1_plotter_units.x == 0
    assert (op.visible_bounds.min.y, op.visible_bounds.max.y) == (10, 40)
    assert op.clip.kind == "rect"
    assert op.clip.bounds.min.x == 20
    assert op.clip.bounds.max.x == 80
    assert state.transform.clip.bounds.max.y == 40
    value = json.loads(doc.to_json())
    assert value["device_profile"]["plot_size_mm"] == [100, 50]
    assert value["pages"][0]["operations"][0]["clip"]["kind"] == "rect"
    del doc, page, profile
    gc.collect()
    assert len(op.geometry.points) == 2
    assert state.transform.rotation == 90
    for obj, attr, value in [(op.clip, "kind", "empty"), (state.transform, "rotation", 0)]:
        with pytest.raises(AttributeError):
            setattr(obj, attr, value)


def test_clip_svg_is_valid_xml_and_wraps_paths_and_dots():
    doc = ezhpgl.parse(b"SP1;IW0,0,400,400;PA-400,200;PD800,200;IW;PA0,0;PD;")
    root = ET.fromstring(doc.to_svg())
    group = root.find(f"{SVG}g")
    clip = group.find(f"{SVG}defs/{SVG}clipPath")
    assert clip.attrib["clipPathUnits"] == "userSpaceOnUse"
    assert clip.find(f"{SVG}rect").attrib == {"x": "0", "y": "0", "width": "10", "height": "10"}
    clipped = group.find(f"{SVG}g")
    assert clipped.attrib["clip-path"] == f"url(#{clip.attrib['id']})"
    assert clipped.find(f"{SVG}path").attrib["d"] == "M-10 5 L20 5"
    assert clipped.find(f"{SVG}circle").attrib["cx"] == "-10"
    assert group.find(f"{SVG}path") is not None
    assert group.find(f"{SVG}circle") is not None


def test_empty_clip_and_default_optional_fields():
    doc = ezhpgl.parse(b"SP1;IW-400,-400,-40,-40;PD400,400;",
                       profile=ezhpgl.DeviceProfile(plot_size_mm=(100, 50)))
    op = doc.pages[0].operations[0]
    assert op.clip.kind == "empty"
    assert op.clip.bounds is None and op.visible_bounds is None
    assert doc.pages[0].bounds is None
    assert ET.fromstring(doc.to_svg()).find(f".//{SVG}path") is None
    doc = ezhpgl.parse(b"SP1;PD40,40;")
    assert doc.final_state.transform is None
    assert doc.device_profile.plot_size_mm is None
    assert doc.pages[0].operations[0].clip is None
    assert "device_profile" not in json.loads(doc.to_json())


@pytest.mark.parametrize("size", [(0, 10), (float("nan"), 10), (10, float("inf")), (300000, 10)])
def test_invalid_profile_is_fatal_even_when_tolerant(size):
    with pytest.raises(ezhpgl.ParseError) as error:
        ezhpgl.parse(b"", strict=False, profile=ezhpgl.DeviceProfile(plot_size_mm=size))
    assert error.value.diagnostic.code == "INVALID_PROFILE"


def test_missing_extent_is_explicit_and_partial_preserves_prior_geometry():
    with pytest.raises(ezhpgl.ParseError) as error:
        ezhpgl.parse(b"RO90;")
    assert error.value.diagnostic.code == "MISSING_DEVICE_BOUNDS"
    doc = ezhpgl.parse(b"SP1;PD40,40;RO90;PD400,400;", strict=False)
    assert doc.completeness == "partial"
    assert doc.stats()["operations"] == 1
    assert (doc.final_state.position.x, doc.final_state.position.y) == (1, 1)


def test_pe_binary_provenance_and_atomic_recovery():
    data = b"SP1;PE\x4f\xc0\xbf\xbf\x4f\xc0;"
    doc = ezhpgl.parse(data, preserve_commands=True)
    op = doc.pages[0].operations[0]
    assert [(p.x, p.y) for p in op.geometry.points] == [(0, 0), (1, 0), (1, 1)]
    assert data[op.source.span.start:op.source.span.end] == doc.raw.commands[1].raw
    bad = ezhpgl.parse(data + b"PE:\xc5\x4f\xc0\xbf<;PD400,400;", strict=False)
    assert bad.diagnostics[0].code == "PE_TRUNCATED"
    assert bad.final_state.pen == 1
    assert bad.stats()["operations"] == 1
    assert (bad.final_state.position.x, bad.final_state.position.y) == (1, 1)


def test_readfile_profile_and_installed_cli_from_other_directory(tmp_path):
    source = tmp_path / "transform.hpgl"
    source.write_bytes(b"RO90;IP;SC0,10,0,10;SP1;PD10,10;")
    profile = ezhpgl.DeviceProfile(plot_size_mm=(100, 50), paper_size_mm=(110, 60))
    doc = ezhpgl.readfile(source, profile=profile)
    assert (doc.final_state.position.x, doc.final_state.position.y) == (0, 50)
    output = tmp_path / "result.svg"
    args = [sys.executable, "-m", "ezhpgl", "export", str(source), "--format", "svg",
            "--plot-size", "100", "50", "--paper-size", "110", "60", "-o", str(output)]
    result = subprocess.run(args, cwd=tmp_path, capture_output=True, text=True)
    assert result.returncode == 0, result.stderr
    assert ET.fromstring(output.read_text()).attrib["data-completeness"] == "complete"
    invalid_output = tmp_path / "invalid.svg"
    args[args.index("100")] = "0"
    args[-1] = str(invalid_output)
    result = subprocess.run(args, cwd=tmp_path, capture_output=True, text=True)
    assert result.returncode == 1
    assert "INVALID_PROFILE" in result.stderr
    assert not invalid_output.exists()


def test_combined_m3_example():
    source = Path(__file__).parents[2] / "examples" / "transformed.hpgl"
    doc = ezhpgl.readfile(source, profile=ezhpgl.DeviceProfile(plot_size_mm=(100, 70)))
    assert len(doc.pages) == 2
    assert doc.completeness == "complete"
    for page in range(2):
        assert ET.fromstring(doc.to_svg(page=page)).find(f".//{SVG}clipPath") is not None
