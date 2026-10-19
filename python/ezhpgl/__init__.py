"""HP-GL parsing, curves, polygon fills, labels, and bounded PCL/PJL extraction."""

from __future__ import annotations

from importlib.metadata import version
from os import PathLike
from pathlib import Path

from . import raw
from ._core import (
    Bounds,
    ClipRegion,
    Contour,
    ContourEdge,
    Curve,
    DeviceProfile,
    Diagnostic,
    FillStyle,
    FontSpec,
    Glyph,
    InputEvent,
    Label,
    TextState,
    Geometry,
    GeometryLimits,
    Limits,
    LineStyle,
    ParseError,
    PlotDocument,
    PlotOperation,
    PlotPage,
    PlotState,
    Point,
    Provenance,
    SourceSpan,
    Style,
    TransformState,
    parse,
)

__version__ = version("ezhpgl")
__all__ = [
    "Bounds",
    "ClipRegion",
    "Contour",
    "ContourEdge",
    "Curve",
    "DeviceProfile",
    "Diagnostic",
    "FillStyle",
    "FontSpec",
    "Glyph",
    "InputEvent",
    "Label",
    "TextState",
    "Geometry",
    "GeometryLimits",
    "Limits",
    "LineStyle",
    "ParseError",
    "PlotDocument",
    "PlotOperation",
    "PlotPage",
    "PlotState",
    "Point",
    "Provenance",
    "SourceSpan",
    "Style",
    "TransformState",
    "__version__",
    "parse",
    "raw",
    "readfile",
]


def readfile(
    path: str | PathLike[str], *, strict: bool = True, preserve_commands: bool = False,
    limits: Limits | None = None, geometry_limits: GeometryLimits | None = None,
    profile: DeviceProfile | None = None,
) -> PlotDocument:
    """Read a plot or supported PCL/PJL container with a bounded binary read, then interpret in Rust."""
    limits = limits if limits is not None else Limits()
    with Path(path).open("rb") as stream:
        data = stream.read(limits.max_input_bytes + 1)
    return parse(data, strict=strict, preserve_commands=preserve_commands,
                 limits=limits, geometry_limits=geometry_limits, profile=profile)
