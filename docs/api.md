# Python API

## Plot API

```python
import ezhpgl

doc = ezhpgl.parse(data, strict=True, preserve_commands=False,
                   limits=ezhpgl.Limits(), geometry_limits=ezhpgl.GeometryLimits(),
                   profile=ezhpgl.DeviceProfile(plot_size_mm=(200, 100)))
doc = ezhpgl.readfile("drawing.hpgl", strict=False, preserve_commands=True)
doc.to_json()                       # JSON string; all pages, schema_version=1
doc.to_svg(page=0, margin_mm=1.0)    # SVG string; one page, white background
doc.write_json("drawing.json")
doc.write_svg("drawing.svg", page=0)
```

`parse` takes bytes. `readfile` accepts a string or `PathLike` and reads at most
`max_input_bytes + 1` bytes before passing the result to Rust. File errors are
`OSError`; parsing and interpretation errors are `ParseError` with a structured
`diagnostic`. Export argument errors are `ValueError` (integer conversion can also
raise `OverflowError`). Invalid page/margin values
are rejected before an output file is opened. File writes replace an existing file.

All model properties are read-only. Lists contain wrappers or value copies;
modifying a returned list does not modify the document. Pages and operations
retain their Rust owner even after the top-level Python object is deleted.

| Object | Fields |
|---|---|
| PlotDocument | schema_version, profile, device_profile, units, completeness, source_size, interpreted_commands, pages, diagnostics, input_events, final_state, raw; stats() |
| DeviceProfile | plot_size_mm and paper_size_mm (optional width/height tuples); validated on parse |
| PlotPage | index (0-based), paper_size_mm, plot_size_mm, bounds, operations, omitted_content |
| PlotOperation | geometry, style, source, path_id, clip, visible_bounds, approximate |
| Geometry | kind="polyline"/"curve"/"polygon"/"label", points, start_dot, curve, contours, fill, label |
| Label | data (bytes), payload_span, terminator, print_terminator, decoded, glyphs, unmapped_offsets, state, start, end, approximate |
| Glyph | character, source_offset, origin, axis_x, axis_y, font, envelope |
| FontSpec | symbol_set, spacing, pitch, height_points, posture, weight, typeface |
| TextState | standard/alternate FontSpec, use_alternate, size_mode, width/height (SI cm or SR percent), direction, relative_direction, origin, slant, extra_space, path, line, transparent, carriage_return (mm) |
| InputEvent | kind, command, span, payload_span (optional); original container offsets |
| Curve | kind="elliptic_arc": center, axis_u, axis_v, start_angle_deg, sweep_angle_deg, chord_mode, chord_value; kind="cubic_bezier": control_points, tolerance_mm |
| Contour | points, edges (retains a shared Rust contour buffer) |
| ContourEdge | start/end inclusive vertex indices, pen_down, source, optional analytic curve |
| FillStyle | fill_type, rule (evenodd/nonzero), spacing_mm, angle_deg, hatch_lines, hatch_phases_mm |
| Point | x, y in mm, except TransformState's explicitly named plotter-unit scaling points |
| Bounds | min/max; sampled clipped centerlines/dot centers, or conservative clipped solid-fill/label envelope; excludes travel outside polygon construction |
| Style | pen (logical ID), color, width_mm, line (optional LineStyle) |
| LineStyle | line_type (None=solid), pattern_length_mm, phase_mm, cap, join, miter_limit |
| Provenance | command_index, command, span (original byte offsets) |
| PlotState | physical position in mm, coordinate_mode (absolute/relative), pen, pen_down, transform, polygon_mode, text (optional TextState), position_approximate |
| TransformState | p1_plotter_units, p2_plotter_units (optional), rotation, scaling, matrix, clip |
| ClipRegion | kind (rect/empty), bounds (None for empty) |

`stats()` returns pages, operations, vertices, interpreted_commands, source_bytes.
Vertices include repeated start points, parameterless PD dots, polygon contour
points, two endpoints per hatch segment and four envelope vertices per emitted glyph.
Control-only labels can have zero vertices. Reusing a polygon counts its vertices
again for each paint operation, even though Rust shares its contour allocation. A one-point
polyline with `start_dot=True` represents a dot. `path_id` connects continuous
vectors across command boundaries, so pen-up travel never joins two strokes.
`raw` is `None` unless preservation is requested; otherwise it is a RawDocument
with the full lexical HP-GL command result, potentially extending beyond interpretation's stop.
For containers, spans address the full original input; container events do not
increment command indices. `input_events` contains the interpreted event prefix.
JSON excludes `raw` even when it was retained in memory.

For labels, `points` is empty; use `label.glyphs` and retain `label.data` for lossless
text bytes. Glyph axes/origins are already physical coordinates. See the
[text/container contract](text-and-containers.md) for placement, decoding,
page transitions and the extraction subset. For polygons, `points` is empty: iterate `contours` instead. For curves, `points`
contains preview vertices and `curve` contains the separate analytic description.
An ellipse is `center + axis_u*cos(t) + axis_v*sin(t)` with t spanning the stored
start/sweep angles in degrees. Axes include SC/RO; do not transform them again.
Absent curve/fill fields are None, unrelated curve fields are None (control_points
is empty for an arc). `hatch_lines` are already intersected with the polygon;
each operation's IW/hard clip still applies. Fixed-pattern hatch phases align
with the hatch lines, and are empty for solid/adaptive line types.

`profile=None` selects unknown extents, using the generic defaults. Both parse and
readfile accept `DeviceProfile`. A present transform describes user-to-mm mapping
as `[a,b,c,d,e,f]`: x'=a*x+c*y+e, y'=b*x+d*y+f. A missing transform is the initial
unscaled 1/40 mapping with unknown P2 and no clip. Plot coordinates are already
transformed: do not apply this final-state matrix to operation vertices again.
Operation geometry is not trimmed; consumers must apply each operation's clip.
See [transform and PE semantics](profile.md) for defaults, reset rules and limits.

Tolerant mode returns the valid prefix at the first lexical error, unsupported
command or invalid semantic argument. It does not resume after IN/DF/BP in the
remaining input. The diagnostic action is `stop_interpretation`; final_state is
the state before the failed command. Resource limits always raise. `complete`
means complete within [the selected profile](profile.md), not device fidelity.

`TEXT_APPROXIMATION`, `UNMAPPED_TEXT_BYTE`, `OMITTED_RASTER` and `OMITTED_PCL_TEXT`
are nonfatal in both modes. They make output partial while interpretation continues.
Check diagnostic actions (`approximate_text`, `replace_glyph`, `omit_content`,
`stop_interpretation`) rather than treating every partial result as a stopped parse.
Warnings are coalesced by code; label and page metadata retain individual details.
Optional JSON fields are omitted when absent or false.
Label bytes are stored as integer arrays in JSON; the full raw source is not embedded.

Geometry limits are separate from the existing lexical `Limits`: max_pages=10,000,
max_operations=1,000,000, max_vertices=2,000,000. They count across the whole
document. Curve and polygon limits are max_curve_segments=65,536 per curve,
max_polygon_vertices=1,000,000 and max_polygon_contours=10,000 per buffer,
max_hatch_lines=100,000 per fill. Text limits are max_label_bytes=65,536 per LB
(including a printed terminator) and max_glyphs=200,000 across the input, including SP0.
Container records share command-count/byte caps; binary headers and data count together. A fill additionally has a fixed 4,000,000
row/edge intersection-work cap per hatch direction. SVG has a separate 2,000,000
vertex/pattern-work budget and raises ValueError before opening a file if exceeded.
Coordinate/range validation applies even to pen-up/unselected movement.
SVG margins must be finite, non-negative and at most 1,000,000 mm. SVG includes
stroke padding; a blank page has a minimum 1 mm content extent.

## Raw API

```python
from ezhpgl import raw

result = raw.parse_commands(data, strict=True, limits=raw.Limits())
commands = raw.iter_commands(data)
detection = raw.detect_format(data)
```

Input is `bytes`; raw filesystem loading belongs to the CLI or caller. `parse_commands`
returns a `RawDocument`, not geometry. Empty input produces an empty complete raw
document; detection reports it as unknown. `iter_commands` parses the entire input
before iteration and retains the source through the iterator. For tolerant mode,
prefer `parse_commands` to retain access to diagnostics.

| Object | Fields |
|---|---|
| DetectionResult | container, language, reasons |
| RawDocument | commands, diagnostics, completeness, source_size; iterable and sized |
| Command | name, kind, args, argument_spans, span, payload_span, raw, data, terminator, print_terminator |
| SourceSpan | start (inclusive), end (exclusive) |
| Diagnostic | code, message, span, command, action |
| ParseError | ValueError subclass; structured diagnostic attribute |

Names are uppercase; raw bytes retain the original casing. Kinds are `numbers`,
`label`, `encoded_polyline`, `comment`, `label_terminator`, or `opaque`. `args` is
populated only for numeric payloads. Label/DT properties are `None` on other kinds.
For LB and PE, `data` excludes the terminator; for CO it excludes double quotes.
DT exposes its effective terminator and printing mode, including DT's default reset.
`span` includes the actual command terminator when consumed. An optional extra
semicolon after LB is padding and is not part of its span.

`DetectionResult` is deliberately heuristic. It reports plain/hpgl_family for a
recognized initial mnemonic, PDF/PostScript signatures, PJL/UEL prefixes, or
pcl_or_control for other leading escapes. It does not scan or extract container
bodies, and it cannot certify the dialect or validity of a whole file.

## Errors and recovery

Strict mode raises on lexical errors or unsupported grammars. Command-specific
numeric arity/ranges are not checked by this layer, except IN/DF, parameterless BP and DT
which affect framing. IN/DF parameter variants and BP kind/value/title arguments
are outside the current lexical profile.
PE payload bytes are framed but neither decoded nor validated as PE coordinates.

Tolerant mode preserves safely semicolon-delimited unsupported commands as opaque
payloads. Numeric corruption can be discarded through an immediately reachable
numeric tail and semicolon. Recovery does not search across arbitrary alphabetic
text, quotes, escapes or binary bytes. Unterminated LB/CO/PE, invalid DT, and unknown
boundaries stop parsing and return only prior commands plus a diagnostic. Every
diagnostic makes the result partial. IN/DF do not update lexical state if invalid.

Resource-limit failures always raise, even in tolerant mode. Defaults:

| Limit | Default |
|---|---:|
| Input bytes | 67,108,864 |
| Bytes per command | 4,194,304 |
| Commands | 1,000,000 |
| Numeric arguments per command | 262,144 |
| Diagnostics | 1,024 |

The byte limit is checked before the binding copies input. Callers should also
bound their own file/network reads. The CLI reads at most its input limit plus one
byte. Limits bound parser work, not all memory already allocated by the caller.
