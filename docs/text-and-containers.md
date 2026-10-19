# Text and bounded PCL/PJL input

The high-level `parse`/`readfile` APIs extract vectors and approximate labels.
They do not emulate a printer page or reproduce native HP fonts. `raw.parse_commands`
still accepts only standalone HP-GL-family streams. All offsets, including container
events and glyph sources, refer to the original byte buffer.

## Label data and fallback placement

`Geometry.kind == "label"` exposes `geometry.label`, with original `data: bytes`,
`payload_span`, delimiter/printing mode, `decoded`, positioned `glyphs`,
`unmapped_offsets`, a pre-label `TextState`, and physical `start`/`end` positions.
`points` is empty for labels. A glyph stores its Unicode character, original byte
offset, font snapshot, physical origin and two signed axes. `axis_x` is nominal
character width; `axis_y` is cap height including slant. These already include RO.
The four-point `envelope` is a conservative fallback preview envelope, not a native
glyph outline or exact ink bound. Spaces have advances but do not contribute ink bounds.

SVG draws individual escaped `<text>` elements with a substitute monospace font.
The renderer's available font determines appearance. It is not an embedded HP
stroke font. Glyphs remain in paint order, use the selected pen color, and respect
IW/hard clipping. LB draws with a solid character style independently of LT, then
restores the previous pen-up/down and line attributes. SP0 suppresses paint but
still processes text movement and resource limits. Labels are unsupported during PM.

| Commands | Supported profile |
|---|---|
| LB / DT | Original single-byte payload; custom terminator and optional printing of that terminator |
| SI / SR | Signed width/height; SI in cm, SR in percent of effective P2−P1 distances; zero sizes allowed |
| DI / DR | Direction in physical axes or relative to P1/P2 distances, followed by RO; zero vector rejected |
| LO | 1–9 and 11–19; left/center/right and bottom/middle/top cap-box placement; 11–19 add a half-cap-height offset |
| SL / ES | Slant tangent and extra cell/line spacing; ES components must be greater than −1 |
| DV | Paths 0 right, 1 down, 2 left, 3 up relative to DI/DR; line direction 0 or 1 |
| CP | No arguments: CR+LF; two arguments: cell spaces and upward line spaces; pen state preserved |
| SD / AD | Standard/alternate font metadata: symbol set, fixed spacing, pitch, height, posture, weight, typeface |
| SS / SA | Select standard/alternate font; normal-mode SI/SO bytes also select alternate/standard |
| TD | 0 normal controls; 1 transparent glyphs, with replacements for unmapped controls |
| SB / CF | Only default SB0 and CF0[,0] forms |

Defaults: Roman-8 (symbol set 277), fixed spacing, 9 characters/inch, 11.5 point
height, upright posture, zero weight, typeface 48, DI1,0, LO1, DV0,0, SL0, ES0,0.
They are explicit generic choices, not detected printer settings. Supported symbol
sets are ASCII 21, Latin-1 14, Roman-8 277 and PC-8 341. Japanese/double-byte fonts,
UTF-8 label interpretation, downloaded fonts, native typeface selection, proportional
spacing, LO21, SB1, other CF variants and FI/FN are unsupported. Posture, weight
and typeface metadata are retained, but SVG uses the same substitute font.

The generic fixed-cell model uses width `25.4/pitch * 2/3` mm and cap height
`height_points * 25.4/72 * 2/3` mm when SI/SR is inactive. Cell advance is 1.5 times
width, line advance twice cap height, multiplied by `1 + ES` components. SI overrides
nominal size; `SI;` returns to selected-font metrics. `SR;` defaults to 0.75%/1.5%.
SC does not scale SI or DI; effective scaling-point distances affect SR and DR.
Missing required extents produce `MISSING_DEVICE_BOUNDS`, without guessed paper sizes.

Placement uses signed nominal cap boxes, independently for each line. Negative
sizes mirror axes and anchors. LO concatenates successive labels only for the
documented path/origin combinations: DV0 with LO1/2/3, DV1 with LO3/6/9, DV2 with
LO7/8/9, DV3 with LO1/4/7 (also their +10 forms). Others restore the incoming
line position. This is a fixed-cell approximation, including mixed-size runs,
whitespace alignment, backspaces, vertical paths and slanted bounds.

Normal-mode CR, LF, BS, HT and SI/SO are recognized. Tabs advance to eight-cell
stops from the carriage-return point; BS cannot move before the first available
character position in that label line. Other C0/C1 controls have no glyph.
Transparent controls and undefined characters use U+FFFD and record original
offsets. Roman-8 byte 255 is blank. A printed DT terminator is processed in the
same mode and its glyph source offset is `payload_span.end`. Normal-mode embedded
`ESC E` is explicitly unsupported, because native printer reset semantics would
otherwise be lost. Escape-looking sequences elsewhere in LB/CO/PE remain payload.

PA/PR, coordinate-bearing PU/PD, arc/Bezier commands, PE, RO, DI/DR/DV/LO and
resets update the text carriage anchor as documented by the generic profile.
CP changes its line component; LB does not reset it, except CR/LF movement inside
the label. IN/DF/BP restore text defaults; DF preserves the physical cursor.

## Approximation and completeness

`TEXT_APPROXIMATION` has action `approximate_text`; `UNMAPPED_TEXT_BYTE` has action
`replace_glyph`. These diagnostics are **nonfatal even with strict=True** and
make the document `partial`. Interpretation continues through supported commands.
They are coalesced by code; per-label raw bytes, unmapped offsets and state remain
available. `Label.approximate` and the label operation's `approximate` are true.

Text movement marks `final_state.position_approximate`. Subsequent relative moves,
curves and polygons inherit that uncertainty. Absolute coordinate positioning
clears cursor uncertainty after the move; a drawn segment beginning at the old
approximate point remains marked. Polygon buffers retain their own uncertainty
even if the cursor is subsequently positioned absolutely. These flags are
conservative dependency markers, not numerical error estimates.

Unsupported state-changing commands, malformed parameters and uncertain framing
still raise in strict mode. Tolerant mode stops at the first such failure with
`stop_interpretation`, preserving only the preceding committed geometry. Resource
limits always raise. `partial` therefore covers both fully consumed approximate
input and stopped input: inspect diagnostic actions and `interpreted_commands`.

## Container profile

This profile extracts HP-GL coordinates; it does not map PCL margins, page size,
picture frames or printer origins. `DeviceProfile` remains the explicit HP-GL plot
extent/clip contract. Unknown commands that could affect these settings stop
interpretation. `PS` is still unsupported.

| Record | Handling |
|---|---|
| UEL (`ESC %-12345X`) | Enter PJL at a command boundary; no implicit page eject/reset |
| PJL COMMENT, empty @PJL | Metadata, requires LF/CRLF line terminator |
| PJL JOB / EOJ | Optional quoted NAME only; EOJ conditionally ends page and resets HP-GL state/DT |
| PJL ENTER LANGUAGE=HPGL2 | Enter direct HP-GL; standalone PG page rules apply |
| PJL ENTER LANGUAGE=PCL | Enter PCL |
| `ESC E` | Conditional page eject and reset, enter PCL |
| `ESC %0B` / `ESC %0A` | Enter HP-GL / PCL, preserving HP-GL cursor and attributes without PCL cursor transfer; omitted zero also accepted |
| PCL FF | Force page boundary, including a blank page; clear HP-GL position/pen-down/path/polygon, preserve attributes/transform/DT |
| PCL `*r#A/B/C/S/T`, `*b#M`, `*t#R` | Recognized raster setup/end controls; omitted-content diagnostic |
| PCL `*b#W` / `*b#V` | Skip exactly the declared unsigned byte count; record original payload span and omitted-content diagnostic |
| Other PCL text | Retain span and report omitted PCL text; ASCII whitespace alone has no ink |

PJL is ASCII; JOB page-selection/settings, other PJL commands/languages and quoted
language values are unsupported. PCL commands must use the non-combined form with
unsigned integer parameters. Cursor-transfer mode 1, negative/standalone switch
variants, modes 2/3, combined lower-case terminators and other binary payload types
stop explicitly. Mode switches require a framed HP-GL command boundary; terminate
numeric commands with a semicolon before an escape. The parser never searches an unrecognized payload for a likely
next mode switch. Declared raster lengths include escape and form-feed bytes as data.

PG is ignored in HP-GL entered from PCL, as required for this mode; PCL FF handles
page breaks. Direct PJL HPGL2 retains standalone PG behavior. Empty trailing state
does not add a phantom page; reset/EOJ finish a page only if it has operations or
omitted content. FF forces blank pages. An entirely empty result has one blank page.

`OMITTED_RASTER` and `OMITTED_PCL_TEXT` (action `omit_content`) are nonfatal,
coalesced diagnostics. Each affected page has `omitted_content=True`; its bounds
describe only retained HP-GL content. A raster-only page can have no bounds.
No raster pixels are decoded. SVG records completeness and omitted-content flags.
`PlotDocument.input_events` holds successfully interpreted container records, in
original order, with kind, command, span and optional payload span. HP-GL operation
command indices refer to the HP-GL commands in `raw`, excluding container events.

## Bounds and limits

The text limit is `GeometryLimits.max_label_bytes=65,536` per label, including a printed
terminator, and `max_glyphs=200,000` across the input, including SP0 text. Each
emitted glyph also consumes four vertices and four SVG work units. Control-only
labels may have zero glyphs/vertices. Container events and HP-GL commands share
`Limits.max_commands`; a binary record's header plus payload must fit
`max_command_bytes`. Input bytes, diagnostics, pages and operations retain existing
caps. Glyph/cursor/envelope coordinates must stay within the generic range.

The synthetic `examples/labels.hpgl` and `examples/mixed-pages.pcl` exercise these
contracts. The mixed example contains deceptive escapes inside a length-framed
raster block. CLI export writes partial output and returns 2 for these examples,
including without `--tolerant`. They are not real CAD/driver samples.
Primary references: HP's [PCL 5 Technical Reference, Part II](https://www.hp.com/ctg/Manual/bpl13211.pdf),
chapter 23 (character commands and label controls), and HP's
[HP-GL/2 and HP RTL Reference Guide](https://people.wou.edu/~soukupm/HPGL2-RTL_ReferenceGuide.pdf),
character commands, printer-mode switching and PG. The bounded fallback and
extraction choices above are this package's contract where native behavior is
device dependent or outside the implemented subset.
