"""Byte-oriented command APIs; complete means lexically complete, not rendered."""

from ._core import (
    Command,
    CommandIterator,
    DetectionResult,
    Diagnostic,
    Limits,
    ParseError,
    RawDocument,
    SourceSpan,
    detect_format,
    parse_commands,
)


def iter_commands(
    data: bytes, *, strict: bool = True, limits: Limits | None = None
) -> CommandIterator:
    """Parse the whole input, then lazily expose command objects.

    This is not incremental input streaming. Use parse_commands() to inspect
    diagnostics and completeness when strict=False.
    """
    return iter(parse_commands(data, strict=strict, limits=limits))


__all__ = [
    "Command", "CommandIterator", "DetectionResult", "Diagnostic", "Limits",
    "ParseError", "RawDocument", "SourceSpan", "detect_format", "parse_commands",
    "iter_commands",
]
