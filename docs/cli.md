# Command-line interface

Installing ezhpgl provides the `ezhpgl` command.

```bash
ezhpgl --version
ezhpgl --help
ezhpgl export --help
```

## Inspect standalone commands

```bash
ezhpgl inspect drawing.hpgl
ezhpgl commands drawing.hpgl --tolerant
```

These commands operate on standalone command syntax. Detection is a prefix hint,
not validation of an entire file. Command spans address the original input bytes.
Use `export` to interpret geometry or supported PCL/PJL containers.

## Export

```bash
ezhpgl export drawing.plt --format json -o drawing.json
ezhpgl export drawing.plt --format svg --page 1 -o page1.svg
ezhpgl export drawing.plt --format svg --page 2 -o page2.svg
ezhpgl export drawing.plt --plot-size 200 100 --paper-size 210 297 \
  --format svg -o drawing.svg
```

JSON contains every page. SVG contains one selected page; CLI page numbers start
at 1, while Python API page indices start at 0. Plot size defines the hard clip
and default scaling points. Paper size is independent metadata, in millimetres.
Existing output files are replaced after parsing and export validation succeed.

`--tolerant` returns the valid prefix when an unsupported or invalid command is
encountered. It does not skip arbitrary commands to complete the drawing.
Use `--max-input-bytes` to cap input size. The Python API also exposes command,
geometry and rendering limits through `Limits` and `GeometryLimits`.

| Exit code | Meaning |
| --- | --- |
| `0` | Complete within the requested parsing or export scope |
| `2` | Partial result; inspect diagnostics and any output written |
| `1` | Parsing, export, argument-value or I/O error |

Known text approximations and raster omissions also make a plot partial, even
without `--tolerant`. Malformed command-line syntax is handled by the argument
parser, which can also exit with code 2 before reading the input.
