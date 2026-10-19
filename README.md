# ezhpgl

A Rust-backed HP-GL and HP-GL/2 parser for Python. Read plot files, inspect their
geometry and source commands, and export JSON or SVG.

- Lines, encoded polylines, arcs, ellipses, Bézier curves, polygons and hatch fills.
- Pen attributes, coordinate transforms, clipping and separate plot pages.
- Labels with original bytes, placement information and substitute-font previews.
- A documented subset of PCL/PJL containers, with diagnostics for omitted content.
- Read-only Python objects, source byte ranges and configurable resource limits.

## Installation

Requires CPython 3.10 or newer. This is an alpha release:

```bash
python -m pip install --pre ezhpgl
```

Conventional GIL-enabled CPython is supported. Building from source requires
Rust 1.85 or newer and maturin; see [development](docs/development.md).

## Quick start

```python
import ezhpgl

plot = ezhpgl.parse(b"IN;SP1;PA0,0;PD1600,0,1600,800,0,800,0,0;PU;")
print(plot.completeness, plot.stats())
plot.write_svg("drawing.svg")
plot.write_json("drawing.json")

# Or read a file:
# plot = ezhpgl.readfile("drawing.plt")
for page in plot.pages:
    for operation in page.operations:
        print(operation.geometry.kind, operation.style.pen, operation.source.span)
```

Coordinates are physical millimetres, with Y pointing up. The generic profile
uses 40 plotter units per millimetre. Paper size and plot extents are unknown
unless supplied explicitly:

```python
profile = ezhpgl.DeviceProfile(plot_size_mm=(200, 100), paper_size_mm=(210, 297))
plot = ezhpgl.readfile("drawing.plt", profile=profile)
```

`plot_size_mm` supplies the hard clipping rectangle and default scaling points.
`paper_size_mm` is separate metadata. Nonzero `RO` and percentage `IR` need known
plot extents. See [profile rules](docs/profile.md).

## Command line

```bash
ezhpgl inspect drawing.hpgl
ezhpgl commands drawing.hpgl --tolerant
ezhpgl export drawing.plt --format svg --page 1 -o drawing.svg
ezhpgl export drawing.plt --format json -o drawing.json
```

`export` interprets geometry and supported containers. `inspect` and `commands`
inspect standalone command syntax. Exit codes are `0` for complete, `2` for a
partial result, and `1` for an error. See [CLI options](docs/cli.md).

## Supported scope and limitations

The parser uses an explicit generic plotter profile. It does not emulate every
printer or recover the original CAD layers, dimensions or model scale.

- `PS` sets the plot extents; the larger value is the X extent (length), as on
  HP devices, and one argument means square extents in this device-independent
  profile. Device-control commands are accepted without
  effect; fill anchors, user line types, tick marks and HP-GL/1 character sets
  are ignored with the nonfatal `IGNORED_COMMAND` diagnostic.
- Text previews use substitute fonts. Native fonts, Japanese/multibyte labels,
  and downloaded fonts are unsupported.
- PCL/PJL support is limited. Supported raster blocks are omitted with diagnostics;
  HP RTL image decoding and full printer emulation are unsupported.
- Unsupported commands raise in strict mode. With `strict=False`, interpretation
  stops at the first uncertain command and returns the valid prefix as `partial`.
- Known text approximations and omissions can also produce `partial` in strict
  mode while interpretation continues. Inspect diagnostic codes and actions.
- A `.plt` extension does not establish the file format. Raw lexical completeness
  does not imply that all commands can be interpreted or rendered.

See the [support matrix](docs/supported-commands.md) for command-level coverage.

## Documentation

- [Python API](docs/api.md)
- [CLI](docs/cli.md)
- [Coordinates, clipping and pages](docs/profile.md)
- [Curves, pens and polygons](docs/drawing.md)
- [Labels and PCL/PJL containers](docs/text-and-containers.md)
- [Architecture](docs/architecture.md)
- [Development](docs/development.md)

## License

[MIT](LICENSE). The parser is independently implemented in Rust with PyO3 bindings.
