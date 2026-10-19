# Support matrix

This matrix separates lexical framing from plot interpretation. All implemented
geometry can be exported as JSON/SVG under the [generic profile](profile.md).

| Family | Lexical support | Plot interpretation |
|---|---|---|
| IN DF SP PA PR PU PD PG | Numeric arguments; IN/DF reset lexical label state | documented arity/ranges and standalone page rules |
| IP IR SC RO IW | Numeric arguments | explicit plot extents for dependent defaults, SC types 0/1/2, rectangular clips |
| PE | Raw bytes through mandatory semicolon | base64/base32, pen/up/absolute/fraction flags, bounded atomic decoding |
| CI AA AR AT RT BZ BR CT | Numeric arguments | analytic curves plus bounded sampling; CT0/1; unsupported infinite-ray degeneracies stop explicitly |
| PW WU PC NP LT LA | Numeric arguments | RGB, pen widths, bounded palette, LT -8…8/99, supported caps/joins; triangular LA variants excluded |
| PM FP EP FT RA RR EA ER WG EW | Numeric arguments | compound polygons, evenodd/nonzero fills, FT1–4, rectangles/wedges; raster/screen fills excluded |
| DT | Immediate terminator byte, optional 0/1 mode, reset | changes label delimiter and optional printed terminator |
| LB | Raw bytes through the current label terminator | byte retention, fixed-cell placement and approximate substitute-font SVG |
| CO | Double-quoted raw comment; embedded semicolons/mnemonics preserved | No drawing effect |
| BP without parameters | Empty command, including BPIN; resets lexical label state | plot-as-you-parse behavior |
| SI SR DI DR LO SL AD SD SS SA CP ES DV TD | Numeric arguments | bounded fixed-cell text profile; 8-bit symbol sets |
| CF SB | Numeric arguments | only default CF0[,0] / SB0 forms |
| FI FN | Numeric arguments | Unsupported |
| AC UL RF TR SV MC CR PS RP VS FS PT TL XT YT CS CA IM DC DP EC AF AH NR OE OA OC OD OF OI OH OO OP OS OW | Numeric framing only | Unsupported |
| BP with parameters, SM, other unknown/unsupported syntax | Strict error; tolerant opaque semicolon-delimited record | Unsupported |
| PCL/PJL containers | Raw parser rejects; high-level framing preserves source offsets | explicit mode-0 and metadata subset; length-framed raster omission, pages |
| HP RTL images / other binary commands | Unknown boundaries stop explicitly | Image decoding unsupported |

Normal syntax accepts upper/lowercase mnemonics, comma/whitespace separators,
sign-separated numbers, fixed-point decimals, and a following mnemonic as a
terminator. A final normal command needs a semicolon or whitespace terminator.
Empty numeric parameters and exponent notation are errors. The raw API does not
generally validate numeric arity or printer clamping rules; the interpreter validates the
supported command forms before state changes. Unsupported semantics stop plot
interpretation even when their lexical framing is supported. Tolerant mode returns
the preceding geometry as partial; it does not silently skip to subsequent lines.
No special payload is decoded as UTF-8 or split by two-letter-looking content.
Supported approximate labels and omitted raster/text records continue with
nonfatal diagnostics, including in strict mode; completeness is partial.
See [text/container contract](text-and-containers.md) for exact variants and limits.

See [drawing contract](drawing.md) for reset rules, resource bounds,
device-profile choices and SVG approximation limits.
