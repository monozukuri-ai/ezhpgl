# Generic plotter profile

`generic-plotter` is an explicit analysis/preview profile for standalone streams,
not a hardware model. `DeviceProfile` supplies optional extents to this profile.
`complete` means that all input was handled under this contract.

| Setting | Contract |
|---|---|
| Coordinates | 40 physical plotter units/mm; default origin (0,0), X right, Y up |
| Initial state | Absolute mode, pen up, pen 0 (unselected) |
| Position range | Each input scalar (current units) and transformed physical position (plotter units) must be within [-8388608,8388607]; decimals retained as f64 |
| Pen selection | SP without arguments equals SP0; 0 draws nothing; nonnegative IDs through 8388607 are rounded to nearest integer, halves upward |
| Pen mapping | Original positive IDs retained until NP explicitly enables a bounded palette |
| Preview style | Initially all positive pens black, 0.35 mm; butt caps, miter joins (limit 5); explicit PD start dots; Pen attribute commands override these defaults |
| Paper size | Unknown unless `DeviceProfile(paper_size_mm=(w,h))` supplies metadata; separate from content and clipping |
| Plot extents | Unknown unless `DeviceProfile(plot_size_mm=(w,h))` supplies a zero-origin hard clip and default P1/P2 |
| IN | Parameterless; absolute mode, physical origin, pen up, rotation 0, scaling off, default P1/P2 and clip; preserves selected pen and existing drawing |
| DF | Parameterless; absolute mode, scaling off, default clip; preserves position, P1/P2, rotation, selected pen, up/down state and existing drawing |
| PA / PR | Change absolute/relative mode; optional X,Y pairs move with current pen state |
| PU / PD | Change pen state; optional X,Y pairs use current coordinate mode |
| CO / DT | Comments have no drawing effect; DT controls the label delimiter and optional printing mode |

The range restriction is this implementation's supported envelope; other devices
may accept different values. Negative pen IDs and device palette/control-panel
behavior are outside the profile. Generic monochrome previews do not establish
original pen colors, physical nib widths or paper boundaries.

## Transforms and clipping

Profile dimensions must be finite, positive and at most 209715.175 mm. Paper size
does not imply plot extents, margins or orientation. Invalid profiles raise
`INVALID_PROFILE` even in tolerant mode. No named hardware defaults are guessed.

`IP` accepts 0, 2 or 4 plotter-unit coordinates, rounding integers with halves
toward positive infinity. Two arguments move P1 and make P2 track it. Coincident
axes are corrected by adding one plotter unit to P2. `IR` has the same arities,
using percentages of the currently oriented plot extents; percentages outside
0–100 are allowed, clamped to [-32768,32767]. No arguments restore default P1/P2.
With unknown extents the default P2 is unknown: use explicit four-argument IP.

`SC xmin,xmax,ymin,ymax[,type]` supports anisotropic type 0 and isotropic type 1.
Type 1 accepts both additional left/bottom percentages together (defaults 50/50;
clamped 0–100), retaining mirrored axes. `SC xmin,xFactor,ymin,yFactor,2` uses
P1 as the anchor and does not need P2. `SC;` disables scaling. Zero ranges/factors
and types outside 0–2 are rejected. Scale/offset components must be finite with magnitude at most
1e12 plotter units; scale magnitudes must be at least 1e-12. Transform arguments
are bounded to magnitude 2^30 before command-specific conversion.

`RO` accepts 0, 90, 180 or 270 degrees, rounded to integer; omitted means 0.
Rotation is not cumulative. It rotates and shifts the origin within known plot
extents; nonzero rotation without extents raises `MISSING_DEVICE_BOUNDS`.
For width W and height H, the physical mappings are (x,y), (W-y,x),
(W-x,H-y), and (y,H-x). P1/P2 keep their numeric values; `IP;`/`IR;` after rotation
restore defaults for the new orientation. Transform changes never move the
physical cursor. Relative moves transform vectors without the origin offset.

`IW x1,y1,x2,y2` orders the corners and defines a soft window in current units.
A window defined while scaling is off ignores IP/IR changes. While scaling is
on it follows IP/IR, until the next SC binds it to the previous scale's plotter
coordinates. RO rotates the soft window. `IW;`, IN and DF restore the hard clip.
The effective window is its intersection with the hard clip, if known. Window
corners are bounded to magnitude 1e9 mm. A disjoint or zero-area window produces
no visible ink in this preview profile.

Operations retain the full transformed path and snapshot their effective clip.
Always apply `operation.clip` when consuming geometry. `None` means unbounded;
`kind="empty"` means fully clipped. Bounds contain visible centerlines/dot centers,
so may be absent even if the edge of a stroke overlaps the window. The cursor
still moves outside the window. SVG clips strokes and dots using clipPath, with
a conservative stroke-inclusive viewport. Window changes do not affect old marks.

## Encoded polylines

PE decodes base64 (8-bit) and base32 (`7`, 7-bit/parity) data, least significant
digit first, with signed-magnitude integers. `:` selects a pen, `<` raises it for
one coordinate pair, `=` makes one pair absolute, and `>` selects fractional bits
in [-26,26]. Flag arguments use the same signed encoding as coordinates, without
coordinate fractional scaling. Coordinates divide by 2^fractional_bits. Spaces,
controls, DEL and the specified ignored high bytes are skipped inside values;
flag high bits are ignored. Base/fraction settings reset for each PE command.

PE defaults to relative coordinates and pen-down moves, then restores the prior
PA/PR mode. The selected pen and last move's up/down state persist. Empty PE and
flag-only configuration preserve up/down state; a dangling `<` or `=` is rejected.
Every number must terminate and coordinates must form pairs. Encoded values are
bounded to 64 bits, and decoded scalars (including flag arguments and pen-up/pen-0
moves) count against `Limits.max_arguments_per_command`. PE output also counts
against the geometry limits. A bad PE commits neither partial geometry nor state.

## Pages

In standalone/direct HP-GL, parameterless `PG;` emits a page only if drawing
operations or known omitted content have been recorded (including fully clipped
operations). `PG n;` emits a
page even if blank; the numeric value does not specify a copy count. More than one
argument is rejected. A semicolon is required (the special PG/RP sequence
is not supported). PG returns the pen to (0,0), raises it, and retains the current
coordinate mode, transforms, clip and selected pen. Marks drawn with pen 0 do not trigger an eject.

Parameterless BP uses plot-as-you-parse behavior: it conditionally emits marked
content, then initializes like IN. It preserves preceding pages. Buffered devices
that discard unprinted content, and BP's kind/value/title options, are not modeled.
An initial BP does not insert a blank first page.

Explicit forced blank pages are preserved. A trailing PG/BP creates no extra
unused page; state-only commands following it also create none. Empty or wholly
non-drawing input with no page eject returns one empty page. The final state is
available independently of whether the next physical sheet was materialized.

SVG uses a white canvas cropped to content plus margin and stroke padding, and
flips Y only at rendering time. It does not assign that canvas size to paper_size_mm.
JSON stores Y-up mm coordinates, ordered operations, source ranges, state and
diagnostics. Schema version 1 gains optional device_profile, plot_size_mm, clip
and transform fields. Default or absent optional fields are omitted. Consumers must honor a present clip field.

## Specification basis and limits

HP's [HP-GL/2 and HP RTL Reference Guide](https://people.wou.edu/~soukupm/HPGL2-RTL_ReferenceGuide.pdf),
printed pages 113–115 (BP), 136–138 (DF), 189–190 (IN), 237–242 (PA/PD),
249–250 (PG), 257–258 (PR), 262–263 (PU), and 306–307 (SP), informs these rules.
The IN/DF distinction, conditional PG, SP0 pen-plotter behavior and millimeter
conversion follow that reference; monochrome mapping and the supported numeric
envelope are explicit implementation choices. Dot marks are represented at
explicit PD starts, including `PD;PU;`.

HP's [PCL 5 Technical Reference, Part II](https://www.hp.com/ctg/Manual/bpl13211.pdf)
describes a different PCL context in which PG is ignored. This package does not
apply standalone PG behavior to HP-GL entered from PCL. The plot API handles bounded
PCL/PJL framing, FF pages, resets and omissions through the separate
[text/container contract](text-and-containers.md).
Transform and PE semantics follow the guide's IP/IR/IW (191–198), PE (243–248), RO (275–278)
and SC (290–295) sections. PE's portable fractional-bit range follows PCL Part II
20-36. Native-device quantization and degenerate clipping behavior are not qualified.
Curves, styles and polygon fills follow the additional [drawing contract](drawing.md).
Labels and bounded containers follow the text/container contract; `PS` sizing and device control commands remain
unsupported and stop interpretation explicitly.
