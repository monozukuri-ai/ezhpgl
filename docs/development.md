# Development

Requirements: CPython 3.10+, Rust 1.85+, and maturin 1.11+.

```bash
python -m venv .venv
# Linux/macOS:
source .venv/bin/activate
# Windows PowerShell: .venv\Scripts\Activate.ps1
python -m pip install "maturin>=1.11,<2" "pytest>=8,<10"
maturin develop --release --locked
python -m pytest
cargo test --workspace --locked
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
```

The Rust workspace is in `crates/`, Python wrappers and stubs in `python/ezhpgl/`,
and tests in `crates/ezhpgl-core/tests/` and `tests/python/`. Small synthetic plot
examples are in `examples/`. Keep command semantics and geometry in the Rust core;
the Python layer supplies I/O and object wrappers.

The version in the Cargo workspace is authoritative. Maturin derives the Python
package version from it, including the conversion of Rust prerelease identifiers
to Python version notation. CPython wheels use the `abi3-py310` stable ABI.
Free-threaded Python, PyPy and WASM are not supported distribution targets.

To build distributions:

```bash
maturin build --release --locked --out dist
maturin sdist --out dist
python scripts/check_dist.py
python scripts/check_public_docs.py
```

These commands build and inspect files locally; they do not upload packages.
