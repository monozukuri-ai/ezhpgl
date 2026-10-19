"""Validate release versions and distribution sets (Python 3.11+, packaging)."""

from __future__ import annotations

import argparse
import hashlib
import json
import tarfile
import tomllib
import zipfile
from email.parser import BytesParser
from pathlib import Path

from check_dist import check as check_payload
from packaging.utils import canonicalize_name, parse_sdist_filename, parse_wheel_filename
from packaging.version import Version

TARGETS = {
    "manylinux_2_28_x86_64",
    "manylinux_2_28_aarch64",
    "win_amd64",
    "macos_x86_64",
    "macos_arm64",
}


def release_version(root: Path, tag: str | None = None) -> Version:
    cargo = tomllib.loads((root / "Cargo.toml").read_text(encoding="utf-8"))
    rust_version = cargo["workspace"]["package"]["version"]
    version = Version(rust_version)
    dependency = cargo["workspace"]["dependencies"]["ezhpgl-core"]["version"]
    if dependency != rust_version:
        raise ValueError("Workspace dependency version does not match the package version")
    lock = tomllib.loads((root / "Cargo.lock").read_text(encoding="utf-8"))
    for name in ("ezhpgl-core", "ezhpgl-python"):
        versions = [p["version"] for p in lock["package"] if p["name"] == name]
        if versions != [rust_version]:
            raise ValueError(f"Cargo.lock version mismatch for {name}: {versions}")
        crate = tomllib.loads((root / "crates" / name / "Cargo.toml").read_text())
        if crate["package"]["version"] != {"workspace": True}:
            raise ValueError(f"{name} must inherit the workspace version")
    project = tomllib.loads((root / "pyproject.toml").read_text(encoding="utf-8"))
    if "version" not in project["project"].get("dynamic", []):
        raise ValueError("The Python version must be derived from Cargo")
    if tag is not None and tag != f"v{version}":
        raise ValueError(f"Expected tag v{version}, got {tag}")
    return version


def wheel_target(tags: set) -> str:
    if any(t.interpreter != "cp310" or t.abi != "abi3" for t in tags):
        raise ValueError("Wheels must use the cp310-abi3 stable ABI")
    platforms = {t.platform for t in tags}
    if platforms == {"manylinux_2_28_x86_64"}:
        return "manylinux_2_28_x86_64"
    if platforms == {"manylinux_2_28_aarch64"}:
        return "manylinux_2_28_aarch64"
    if platforms == {"win_amd64"}:
        return "win_amd64"
    for arch in ("x86_64", "arm64"):
        if platforms and all(p.startswith("macosx_") and p.endswith(f"_{arch}")
                             for p in platforms):
            return f"macos_{arch}"
    raise ValueError(f"Unexpected wheel platforms: {sorted(platforms)}")


def check_metadata(data: bytes, version: Version) -> None:
    metadata = BytesParser().parsebytes(data)
    if canonicalize_name(metadata["Name"] or "") != "ezhpgl":
        raise ValueError("Distribution name must be ezhpgl")
    if Version(metadata["Version"] or "0") != version:
        raise ValueError("Distribution metadata version mismatch")
    if metadata["Requires-Python"] != ">=3.10":
        raise ValueError("Unexpected Requires-Python")
    if metadata["License-Expression"] != "MIT":
        raise ValueError("Expected MIT license metadata")
    for requirement in metadata.get_all("Requires-Dist", []):
        if "@" in requirement:
            raise ValueError(f"Direct dependency URL in distribution: {requirement}")


def check_artifacts(directory: Path, version: Version, *, complete: bool = False) -> list[Path]:
    paths = sorted(directory.glob("*.whl")) + sorted(directory.glob("*.tar.gz"))
    if not paths:
        raise ValueError("No distribution artifacts found")
    targets: list[str] = []
    sdists = 0
    for path in paths:
        if path.suffix == ".whl":
            name, file_version, build, tags = parse_wheel_filename(path.name)
            if build:
                raise ValueError("Unexpected wheel build tag")
            targets.append(wheel_target(tags))
            with zipfile.ZipFile(path) as archive:
                entries = [n for n in archive.namelist() if n.endswith(".dist-info/METADATA")]
                if len(entries) != 1:
                    raise ValueError("Expected exactly one wheel METADATA")
                check_metadata(archive.read(entries[0]), version)
        else:
            name, file_version = parse_sdist_filename(path.name)
            sdists += 1
            with tarfile.open(path) as archive:
                check_metadata(archive.extractfile(f"ezhpgl-{version}/PKG-INFO").read(), version)
                manifest = archive.extractfile(f"ezhpgl-{version}/Cargo.toml").read().decode()
                if Version(tomllib.loads(manifest)["workspace"]["package"]["version"]) != version:
                    raise ValueError("Source archive Cargo version mismatch")
        if canonicalize_name(name) != "ezhpgl" or file_version != version:
            raise ValueError(f"Distribution filename version/name mismatch: {path.name}")
        check_payload(path)
    if len(set(targets)) != len(targets) or sdists > 1:
        raise ValueError("Duplicate release target or source distribution")
    if complete and (set(targets) != TARGETS or sdists != 1):
        raise ValueError(f"Incomplete release set: wheels={targets}, sdists={sdists}")
    return paths


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[1])
    parser.add_argument("--tag")
    parser.add_argument("--github-output", type=Path)
    parser.add_argument("--dist", type=Path)
    parser.add_argument("--complete", action="store_true")
    parser.add_argument("--checksums", type=Path)
    args = parser.parse_args()
    if (args.complete or args.checksums) and not args.dist:
        parser.error("--complete and --checksums require --dist")
    version = release_version(args.root, args.tag)
    outputs = {"version": str(version), "tag": f"v{version}",
               "prerelease": str(version.is_prerelease).lower()}
    print(json.dumps(outputs))
    if args.github_output:
        with args.github_output.open("a", encoding="utf-8") as stream:
            stream.writelines(f"{key}={value}\n" for key, value in outputs.items())
    if args.dist:
        paths = check_artifacts(args.dist, version, complete=args.complete)
        if args.checksums:
            args.checksums.write_text("".join(
                f"{hashlib.sha256(path.read_bytes()).hexdigest()}  {path.name}\n"
                for path in paths
            ), encoding="utf-8")


if __name__ == "__main__":
    main()
