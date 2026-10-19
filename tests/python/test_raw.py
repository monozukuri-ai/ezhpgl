from __future__ import annotations

import gc
import importlib.metadata
import json
import subprocess
import sys
from pathlib import Path

import pytest

import ezhpgl
from ezhpgl import raw


def test_installed_version_and_api():
    assert ezhpgl.__version__ == importlib.metadata.version("ezhpgl")
    result = raw.parse_commands(b"INSP1PA0,0PD40,-20;")
    assert result.completeness == "complete"
    assert result.diagnostics == []
    assert len(result) == 4
    assert [c.name for c in result] == ["IN", "SP", "PA", "PD"]
    assert result.commands[-1].args == [40, -20]


def test_binary_payload_offsets_and_shared_lifetimes():
    data = b' \nDT#,0;LB;PU\xff#;CO"PE;IN";PEAB\x80\xff;'
    result = raw.parse_commands(data)
    commands = result.commands
    iterator = iter(result)
    del result
    gc.collect()
    assert next(iterator).name == "DT"
    assert commands[1].data == b";PU\xff"
    assert commands[1].terminator == ord("#")
    assert commands[1].print_terminator is True
    assert commands[2].data == b"PE;IN"
    assert commands[3].data == b"AB\x80\xff"
    for command in commands:
        assert command.raw == data[command.span.start:command.span.end]
        assert command.data == data[command.payload_span.start:command.payload_span.end]


def test_argument_spans():
    data = b"PA-.5+20.0;"
    command = raw.parse_commands(data).commands[0]
    assert command.args == [-0.5, 20.0]
    assert [data[s.start:s.end] for s in command.argument_spans] == [b"-.5", b"+20.0"]


def test_objects_are_read_only():
    command = raw.parse_commands(b"SP1;").commands[0]
    with pytest.raises(AttributeError):
        command.name = "PU"
    with pytest.raises(AttributeError):
        command.span.start = 3


def test_structured_error_and_tolerant_recovery():
    data = b"IN;PA1e3,2;SP2;"
    with pytest.raises(raw.ParseError) as info:
        raw.parse_commands(data)
    d = info.value.diagnostic
    assert isinstance(info.value, ValueError)
    assert (d.code, d.command, d.span.start) == ("EXPONENT_NOT_ALLOWED", "PA", 5)
    result = raw.parse_commands(data, strict=False)
    assert result.completeness == "partial"
    assert [c.name for c in result] == ["IN", "SP"]
    assert result.diagnostics[0].action == "skip_command"


@pytest.mark.parametrize("data,code", [
    (b"LBhello;SP1;", "UNTERMINATED_LABEL"),
    (b"PEabcIN", "UNTERMINATED_PE"),
    (b'CO"abc;IN;', "UNTERMINATED_COMMENT"),
    (b"DT#,2;LBx#;", "INVALID_DT"),
])
def test_no_recovery_across_unknown_payload_boundary(data, code):
    result = raw.parse_commands(b"IN;" + data, strict=False)
    assert [c.name for c in result] == ["IN"]
    assert result.diagnostics[0].code == code
    assert result.diagnostics[0].action == "stop"


def test_iter_commands_retains_buffer():
    iterator = raw.iter_commands(b"IN;SP1;PU;")
    assert iter(iterator) is iterator
    assert [c.name for c in iterator] == ["IN", "SP", "PU"]
    assert list(iterator) == []


@pytest.mark.parametrize("data,container", [
    (b"IN;", "plain"), (b"%PDF-1.7", "pdf"), (b"%!PS", "postscript"),
    (b"\x1b%-12345X@PJL", "pjl"), (b"\x1b%0B", "pcl_or_control"), (b"", "unknown"),
])
def test_detection(data, container):
    result = raw.detect_format(data)
    assert result.container == container
    assert result.reasons


def test_container_is_rejected_even_in_tolerant_mode():
    with pytest.raises(raw.ParseError) as info:
        raw.parse_commands(b"\x1b%0BIN;", strict=False)
    assert info.value.diagnostic.code == "UNSUPPORTED_CONTAINER"


def test_limits_are_not_suppressed():
    limits = raw.Limits(max_input_bytes=3, max_commands=1)
    assert limits.max_input_bytes == 3
    with pytest.raises(raw.ParseError) as info:
        raw.parse_commands(b"IN;SP1;", strict=False, limits=limits)
    assert info.value.diagnostic.code == "LIMIT_EXCEEDED"
    with pytest.raises((OverflowError, ValueError)):
        raw.Limits(max_commands=-1)


def test_bytes_are_required():
    with pytest.raises(TypeError):
        raw.parse_commands("IN;")


def test_cli_uses_installed_package_from_another_directory(tmp_path):
    source = tmp_path / "drawing.hpgl"
    source.write_bytes(b"IN;SP1;DT#;LB;PD#;")
    result = subprocess.run(
        [sys.executable, "-m", "ezhpgl", "inspect", str(source)],
        cwd=tmp_path, capture_output=True, text=True,
    )
    assert result.returncode == 0, result.stderr
    output = json.loads(result.stdout)
    assert output["scope"] == "lexical"
    assert output["command_counts"] == {"DT": 1, "IN": 1, "LB": 1, "SP": 1}


@pytest.mark.parametrize("data,tolerant,exit_code", [
    (b"IN;ZZ;", True, 2), (b"IN;ZZ;", False, 1),
    (b"\x1b%0BIN;", True, 1), (b"IN;", False, 0),
])
def test_cli_exit_status(tmp_path, data, tolerant, exit_code):
    source = tmp_path / "input.plt"
    source.write_bytes(data)
    command = [sys.executable, "-m", "ezhpgl", "commands", str(source)]
    if tolerant:
        command.append("--tolerant")
    result = subprocess.run(command, capture_output=True, text=True)
    assert result.returncode == exit_code
    json.loads(result.stdout if exit_code != 1 else result.stderr)


def test_cli_input_limit(tmp_path):
    source = tmp_path / "input.hpgl"
    source.write_bytes(b"IN;SP1;")
    result = subprocess.run(
        [sys.executable, "-m", "ezhpgl", "inspect", str(source), "--max-input-bytes", "3"],
        capture_output=True, text=True,
    )
    assert result.returncode == 1
    assert json.loads(result.stderr)["error"]["code"] == "LIMIT_EXCEEDED"


def test_fixture_manifest():
    directory = Path(__file__).parents[1] / "fixtures"
    manifest = json.loads((directory / "manifest.json").read_text())
    for fixture in manifest:
        data = (directory / fixture["file"]).read_bytes()
        assert fixture["kind"] == "synthetic"
        if "error" in fixture:
            with pytest.raises(raw.ParseError) as info:
                raw.parse_commands(data)
            assert info.value.diagnostic.code == fixture["error"], fixture["file"]
        else:
            result = raw.parse_commands(data)
            assert [c.name for c in result] == fixture["commands"], fixture["file"]
            assert result.completeness == "complete"


def test_raw_command_golden():
    directory = Path(__file__).parents[1] / "golden"
    data = (directory / "commands.hpgl").read_bytes()
    expected = json.loads((directory / "commands.json").read_text())
    result = raw.parse_commands(data)
    actual = [
        {"name": c.name, "kind": c.kind, "args": c.args,
         "span": [c.span.start, c.span.end],
         "payload_span": [c.payload_span.start, c.payload_span.end],
         "data_hex": c.data.hex()}
        for c in result
    ]
    assert actual == expected
