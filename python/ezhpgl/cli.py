"""Inspect raw HP-GL commands or export interpreted geometry."""

from __future__ import annotations

import argparse
import json
import sys
from collections import Counter
from pathlib import Path

from . import DeviceProfile, __version__, raw, readfile


def _diagnostic(d: raw.Diagnostic) -> dict:
    return {
        "code": d.code, "message": d.message,
        "span": [d.span.start, d.span.end], "command": d.command, "action": d.action,
    }


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(prog="ezhpgl")
    parser.add_argument("--version", action="version", version=__version__)
    subparsers = parser.add_subparsers(dest="operation", required=True)
    for name in ("inspect", "commands", "export"):
        sub = subparsers.add_parser(name)
        sub.add_argument("path", type=Path)
        sub.add_argument("--tolerant", action="store_true")
        sub.add_argument("--max-input-bytes", type=int, default=raw.Limits().max_input_bytes)
        if name == "export":
            sub.add_argument("--format", choices=("json", "svg"), required=True)
            sub.add_argument("-o", "--output", type=Path, required=True)
            sub.add_argument("--page", type=int, help="SVG page number (1-based; default 1)")
            sub.add_argument("--plot-size", type=float, nargs=2, metavar=("WIDTH", "HEIGHT"),
                             help="Zero-origin hard clipping area in mm")
            sub.add_argument("--paper-size", type=float, nargs=2, metavar=("WIDTH", "HEIGHT"),
                             help="Paper metadata in mm (does not define the clipping area)")
    args = parser.parse_args(argv)
    if args.max_input_bytes < 0:
        parser.error("--max-input-bytes must be non-negative")
    if args.operation == "export" and args.page is not None:
        if args.page < 1 or args.format != "svg":
            parser.error("--page requires SVG and a positive 1-based page number")
    try:
        if args.operation == "export":
            result = readfile(args.path, strict=not args.tolerant,
                              profile=DeviceProfile(
                                  plot_size_mm=tuple(args.plot_size) if args.plot_size else None,
                                  paper_size_mm=tuple(args.paper_size) if args.paper_size else None),
                              limits=raw.Limits(max_input_bytes=args.max_input_bytes))
            if args.format == "svg":
                result.write_svg(args.output, page=(args.page or 1) - 1)
            else:
                result.write_json(args.output)
            print(json.dumps({"completeness": result.completeness, "stats": result.stats(),
                              "diagnostics": [_diagnostic(d) for d in result.diagnostics]}))
            return 0 if result.completeness == "complete" else 2
        # Bound reads even if the file grows after stat(), and avoid an unlimited read.
        with args.path.open("rb") as stream:
            data = stream.read(args.max_input_bytes + 1)
        detection = raw.detect_format(data)
        result = raw.parse_commands(
            data, strict=not args.tolerant,
            limits=raw.Limits(max_input_bytes=args.max_input_bytes),
        )
        output = {
            "version": __version__, "scope": "lexical",
            "container": detection.container, "language": detection.language,
            "detection_reasons": detection.reasons,
            "completeness": result.completeness,
            "input_bytes": result.source_size,
            "command_counts": dict(sorted(Counter(c.name for c in result).items())),
            "diagnostics": [_diagnostic(d) for d in result.diagnostics],
        }
        if args.operation == "commands":
            output["commands"] = [
                {"name": c.name, "kind": c.kind, "args": c.args,
                 "span": [c.span.start, c.span.end],
                 "payload_span": [c.payload_span.start, c.payload_span.end],
                 "data_hex": c.data.hex(), "terminator": c.terminator,
                 "print_terminator": c.print_terminator}
                for c in result
            ]
        print(json.dumps(output, ensure_ascii=True, indent=2))
        return 0 if result.completeness == "complete" else 2
    except raw.ParseError as error:
        print(json.dumps({"error": _diagnostic(error.diagnostic)}), file=sys.stderr)
        return 1
    except (OSError, ValueError) as error:
        print(str(error), file=sys.stderr)
        return 1
