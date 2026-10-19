# Architecture

`ezhpgl-core` owns input detection, command parsing, interpretation, geometry,
source ranges, diagnostics and JSON/SVG serialization. `ezhpgl-python` exposes
those Rust objects through PyO3. The Python package handles file I/O and the CLI.
The parser has no dependency on ezdxf or a CAD conversion framework.

## Input and ownership

The parser consumes a complete byte buffer. Commands retain ranges into that
buffer, preserving original labels and encoded-polylines without decoding the
whole input as text. Numeric values are finite `f64` values; their original
lexemes remain accessible through argument spans.

A bounded lexical pass precedes interpretation. Container handling cooperates
with the lexer at command boundaries and consumes supported binary payloads by
length. It does not search label, comment or raster payloads for instructions.
The raw command API handles standalone streams; the plot API also handles the
[documented container subset](text-and-containers.md).

Python wrappers retain shared ownership of their Rust document. Pages and
operations remain valid after the original Python document variable is deleted.
Accessing raw payload bytes creates a copy of the selected range. Parsing and
serialization release Python's GIL.

## Plot model

Each page stores ordered drawing operations in physical millimetres. Each
operation has the pen attributes, effective clip and source command that applied
when it was created. Later state changes do not alter earlier operations.

Analytic arcs, ellipses and cubic Béziers are retained separately from bounded
preview vertices. Polygon contours retain pen-up edges and source references.
Labels retain bytes and positioned substitute-font glyphs; text-dependent
geometry carries approximation information.

Commands commit their state and geometry only after validation. Unsupported
state changes stop interpretation. Tolerant parsing returns the preceding valid
geometry and a diagnostic; it does not guess how to resume through unknown state.
Resource-limit failures always raise.

## Export

JSON represents the document, pages, operations and diagnostics. It includes
label bytes but does not embed the complete raw input buffer.

SVG renders one page at a time. It preserves paint order, applies per-operation
clips and flips the Y axis for display. Its viewport fits the visible content
with stroke padding and a requested margin; it is not a paper-size declaration.
Both exports retain the distinction between complete and partial interpretation.
