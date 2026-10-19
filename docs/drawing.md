# Curves, pen attributes and polygons

These rules extend the [generic plotter profile](profile.md). They describe an
analysis and SVG preview contract, not emulation of a particular plotter.
All geometry is in physical millimeters after SC/RO, with per-operation clipping.

## Curves

`CI radius[,chord]` draws independently of the incoming pen state and restores
the center and up/down state. A negative radius starts at 180 degrees; zero
produces a dot. `AA/AR` use absolute/relative centers and signed sweeps;
`AT/RT` pass through an intermediate point and endpoint. These arcs follow the
current pen state. Relative three-point coordinates are relative to the original
start, not to the intermediate point. Their endpoints become the current position.
Anisotropic or mirrored SC maps circles to analytic ellipses with signed axes.

Sweeps for AA/AR are clamped to [-32768,32767], then reduced modulo 360 when their
magnitude exceeds 360. AT/RT coincident intermediate points or a collinear
intermediate point between start/end produce a line. A coincident start/end with
a distinct intermediate point defines a full circle with that diameter. Collinear
cases requiring infinite rays raise `UNSUPPORTED_GEOMETRY`.

`CT0` (default) interprets chord values as degrees, default 5. Values are normalized
modulo 360, reflected above 180 and bounded below by 0.5 degrees. `CT1` interprets
positive chord deviation in current units, also default 5. Equal subdivisions use
the ceiling of sweep/step, subject to the curve limit. The analytic shape and
original chord mode/value remain available separately from preview vertices.

`BZ/BR` take groups of six values for cubic control points and endpoints. BR's
three points are relative to the start of each cubic. Adaptive subdivision uses
a 0.01 mm control-hull flatness tolerance, depth at most 24, and the curve segment
limit. A malformed or excessive later cubic rejects the whole command.

## Pen attributes

`PC pen,r,g,b` snapshots clamped 0–255 RGB. `PC pen` resets that pen; `PC;` resets
all colors to the generic black default. No device-specific default palette is
inferred. `NP n` enables a pen-plotter palette of 1…N, rounding up to a power of
two with a minimum of 2 and maximum of 256. NP without arguments selects 8.
Positive SP values then wrap through that palette; SP0 remains unselected.
Shrinking drops removed pen entries, and growing adds generic defaults. At most
256 explicit color overrides and 256 explicit width overrides are retained.

`PW width[,pen]` assigns one pen or all pens. `PW;` restores default widths.
`WU0` uses mm with a default of 0.35; `WU1` uses percentages of the effective
P1/P2 diagonal with a default of 0.1%. Each WU resets width settings. Relative
widths and relative line/hatch defaults require known P1/P2. The generic minimum
width, including PW0 hairlines, is one plotter unit (0.025 mm).

`LT` supports solid, type 0 endpoint marks, fixed types 1…8 and adaptive types
-1…-8. Length defaults to 4% of the effective diagonal; mode 1 supplies mm.
Omitted length/mode retain their previous values. Fixed patterns carry a residue
across consecutive drawing vectors, including pen-up travel returning to the last
endpoint. `LT;` saves the pattern/residue and selects solid; `LT99` restores it
when still solid and at the saved position. Transform/pen/width/line-attribute
changes invalidate continuity. Adaptive patterns fit an integer number of
repetitions into each vector or curve chord. Dots use a one-plotter-unit mark.

`LA` supports butt/square/round caps (1/2/4), miter/miter-bevel/round/bevel/no-join
joins (1/2/4/5/6), and a miter limit clamped to 1…32767. Triangular cap/join 3 raises
`UNSUPPORTED_ATTRIBUTE`. SVG uses its miter fallback for joins 1/2. No-join strokes
are emitted as individual segments. Physical-device nib and raster quantization
are not modeled.

IN/BP reset pen, curve and fill settings and polygon buffer. DF resets LT/LA/CT/FT and the
polygon buffer, preserving color, width, WU and NP. Neither clears earlier marks.
PG clears the polygon buffer and path residue but retains pen/fill attributes.

## Polygons and fills

`PM0` clears the polygon buffer and enters polygon mode at the current position.
PM1 closes the current contour; PM2 closes it and exits. Implicit closure is a
pen-up edge and moves the cursor to the contour's first vertex. The first straight
coordinate after PM1 starts a new contour; curve commands retain their start.
The current up/down state persists on exit. No drawing occurs while buffering.
Both pen-up and pen-down edges are retained. PE pen-selection flags are ignored
within polygon mode, but malformed PE remains an error.

CI closes any preceding contour, creates a complete circle contour and restores
its center. Subsequent coordinates start a new contour. Closing a preceding open
contour can therefore change the CI center. FP/EP within polygon mode have no
effect. State changes outside the supported drawing subset are rejected there.
An open polygon at end of input raises `UNCLOSED_POLYGON`, or returns a partial
result with `final_state.polygon_mode=True` in tolerant mode.

`FP0`/`FP;` use the even-odd rule; `FP1` uses nonzero winding. All contour vertices,
including pen-up moves, define fill boundaries. `EP` strokes only pen-down edges.
The buffer persists for repeated FP/EP; each painting command snapshots current
attributes and clipping. Contours with fewer than three stored points do not paint.
Geometrically degenerate solid fills become lines or dots.

`EA/ER` edge absolute/relative rectangles, `RA/RR` fill them. `EW/WG` edge/fill
wedges with radius, start angle, sweep and optional chord. Sweep is clamped to
±360, negative radii shift start by 180, and full circles omit radial edges.
These commands replace the polygon buffer and restore cursor and up/down state;
subsequent FP/EP can repaint their shapes.

`FT1/2` are solid fills. FT3 is hatch and FT4 crosshatch. Spacing zero defaults to
1% of the effective P1/P2 diagonal; explicit spacing is positive current X units.
Unscaled spacing stays fixed; scaled spacing tracks SC/IP until SC is disabled,
which freezes it physically. FT3/4 remember separate spacing/angle settings.
Angles are measured from plotter X including RO; crosshatching adds 90 degrees.
The default anchor is the rotated plotter origin. Fixed line patterns align to
that anchor and alternate a half-period shift between rows; adaptive patterns
fit each interior hatch segment. Hatch endpoints use the current line caps and
are subject to IW, so wide caps can extend slightly beyond a polygon boundary.
FT10/11/21/22 and custom anchors/patterns are unsupported and stop interpretation.

## Output and limits

JSON/Python retain analytic curves, sampled contour geometry, construction source
ranges, fill rules, hatch segments and style snapshots. SVG paints sampled paths
in operation order and applies the stored clip. Solid fills use SVG compound
paths with the stored fill rule. Bounds for lines/arcs/hatches describe sampled
visible centerlines; solid-fill bounds are a conservative clipped contour envelope
and need not exclude holes or concavities. They are not exact analytic CAD bounds.

SVG joins adjacent operations only when their path IDs and attributes match.
Pen-up commands and isolated shape paints split SVG paths, even if a later stroke
returns to the same point; fixed-pattern phase is preserved where specified.
Caps/joins at such boundaries are a preview limitation. Curve chords, generic
palette defaults, minimum width and SVG line-join behavior are explicit preview
choices; `complete` does not certify native-device appearance.

See [API limits](api.md) for curve, polygon, hatch and SVG work budgets. Limits
always fail, including in tolerant mode. The raw API and default line-only JSON
golden remain compatible; schema 1 gains optional fields and new geometry kinds.
Consumers that only handle polylines must dispatch on `geometry.kind`.

The implementation follows HP's [HP-GL/2 and HP RTL Reference Guide](https://people.wou.edu/~soukupm/HPGL2-RTL_ReferenceGuide.pdf),
the Polygon Group (pages 45–53), and the individual AA/AR/AT/BR/BZ/CI/CT,
EA/EP/ER/EW/FP/FT/LA/LT/NP/PC/PM/PW/RA/RR/RT/WG/WU entries, together with HP's
[PCL 5 Technical Reference, Part II](https://www.hp.com/ctg/Manual/bpl13211.pdf).
Device-dependent choices and unsupported variants are narrowed above.
