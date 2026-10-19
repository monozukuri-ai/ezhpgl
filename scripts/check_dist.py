"""Check wheel/sdist content boundaries and metadata without installing them."""

from __future__ import annotations

import argparse
import configparser
import tarfile
import zipfile
from pathlib import Path, PurePosixPath


def check(path: Path) -> None:
    if path.suffix == ".whl":
        with zipfile.ZipFile(path) as archive:
            names = archive.namelist()
            assert "ezhpgl/_core.pyi" in names
            assert "ezhpgl/py.typed" in names
            assert "ezhpgl/raw.py" in names
            assert "ezhpgl/cli.py" in names
            assert any(name.endswith((".so", ".pyd")) for name in names)
            assert any(name.endswith("/licenses/LICENSE") for name in names)
            metadata = archive.read(next(n for n in names if n.endswith(".dist-info/METADATA"))).decode()
            assert "Requires-Python: >=3.10" in metadata
            assert "Name: ezhpgl\n" in metadata
            entry_points = configparser.ConfigParser()
            entry_points.read_string(archive.read(
                next(n for n in names if n.endswith(".dist-info/entry_points.txt"))
            ).decode())
            assert entry_points["console_scripts"]["ezhpgl"] == "ezhpgl.cli:main"
    else:
        with tarfile.open(path) as archive:
            names = archive.getnames()
            assert any(name.endswith("/crates/ezhpgl-core/src/lexer.rs") for name in names)
            assert any(name.endswith("/crates/ezhpgl-core/src/interpreter.rs") for name in names)
            assert any(name.endswith("/crates/ezhpgl-core/src/export.rs") for name in names)
            for module in ("transform", "clip", "pe", "curves", "attributes", "polygon", "interpreter/drawing", "text", "stream", "interpreter/labels"):
                assert any(name.endswith(f"/crates/ezhpgl-core/src/{module}.rs") for name in names)
            assert any(name.endswith("/crates/ezhpgl-python/src/lib.rs") for name in names)
            assert any(name.endswith("/Cargo.lock") for name in names)
            assert any(name.endswith("/LICENSE") for name in names)
            assert any(name.endswith("/tests/fixtures/manifest.json") for name in names)
            assert any(name.endswith("/tests/golden/plot.json") for name in names)
            assert any(name.endswith("/examples/linework.hpgl") for name in names)
            assert any(name.endswith("/examples/transformed.hpgl") for name in names)
            assert any(name.endswith("/tests/python/test_transforms.py") for name in names)
            assert any(name.endswith("/tests/python/test_drawing.py") for name in names)
            assert any(name.endswith("/crates/ezhpgl-core/tests/drawing.rs") for name in names)
            assert any(name.endswith("/examples/curves-and-fills.hpgl") for name in names)
            for required in ("crates/ezhpgl-core/tests/labels.rs", "crates/ezhpgl-core/tests/containers.rs",
                             "crates/ezhpgl-python/src/plot/labels.rs", "tests/python/test_labels_containers.py",
                             "docs/text-and-containers.md", "docs/cli.md", "docs/development.md",
                             "examples/labels.hpgl", "examples/mixed-pages.pcl"):
                assert any(name.endswith("/" + required) for name in names)
            prefix = names[0].split("/", 1)[0]
            license_text = archive.extractfile(f"{prefix}/LICENSE").read()
            for crate in ("ezhpgl-core", "ezhpgl-python"):
                assert archive.extractfile(f"{prefix}/crates/{crate}/LICENSE").read() == license_text
    forbidden = {".internal", ".agents", ".codex", ".aws", ".git", "__pycache__", "target", ".pytest_cache", ".ruff_cache", "AGENTS.md"}
    private_tools = {"fetch_corpus.py", "audit_corpus.py", "audit_pe_corpus.py",
                     "check_cad2d_integration.py", "compare_lines.py", "compare_curves.py",
                     "compare_transforms.py", "check_text_render.py", "check_drawing_render.py"}
    for name in names:
        parts = PurePosixPath(name).parts
        assert not (forbidden & set(parts)), (path, name)
        assert parts[-1] not in private_tools, (path, name)
        assert not any(p.startswith(".venv") for p in parts), (path, name)
        assert not name.endswith(".plt"), (path, "external plot sample", name)
    print(f"PASS {path.name}: {len(names)} entries, public payload only")


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("paths", type=Path, nargs="*")
    args = parser.parse_args()
    paths = args.paths or sorted(Path("dist").glob("*.whl")) + sorted(Path("dist").glob("*.tar.gz"))
    if not paths:
        parser.error("No distribution artifacts found")
    for path in paths:
        check(path)


if __name__ == "__main__":
    main()
