"""Release gates fail before upload when versions or artifact contracts drift."""

import shutil
import sys
from pathlib import Path

import pytest
from packaging.tags import Tag
from packaging.version import Version

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts"))

from check_public_docs import check as check_docs  # noqa: E402
from check_release import check_artifacts, check_metadata, release_version, wheel_target  # noqa: E402


def test_current_version_and_docs():
    version = release_version(ROOT)
    assert release_version(ROOT, f"v{version}") == version
    assert check_docs(ROOT)[0] >= 1


@pytest.mark.parametrize("tag", ["v9.9.9", "v0.1.0-alpha.1", "0.1.0a1"])
def test_reject_wrong_tag(tag):
    with pytest.raises(ValueError, match="Expected tag"):
        release_version(ROOT, tag)


def test_reject_stale_lock(tmp_path):
    for name in ("Cargo.toml", "Cargo.lock", "pyproject.toml"):
        shutil.copyfile(ROOT / name, tmp_path / name)
    shutil.copytree(ROOT / "crates", tmp_path / "crates")
    lock = tmp_path / "Cargo.lock"
    lock.write_text(lock.read_text().replace('version = "0.1.0-alpha.1"', 'version = "9.9.9"'))
    with pytest.raises(ValueError, match="Cargo.lock version mismatch"):
        release_version(tmp_path)


@pytest.mark.parametrize("tag", [Tag("cp314", "cp314", "win_amd64"),
                                Tag("cp310", "abi3", "linux_x86_64"),
                                Tag("cp310", "abi3", "win32")])
def test_reject_incompatible_wheel(tag):
    with pytest.raises(ValueError):
        wheel_target({tag})


@pytest.mark.parametrize("field", ["Version: 9.9.9", "Name: another-package",
                                   "Requires-Python: >=3.14", "License-Expression: GPL-3.0"])
def test_reject_metadata_mismatch(field):
    data = {"Name": "ezhpgl", "Version": "0.1.0a1", "Requires-Python": ">=3.10",
            "License-Expression": "MIT"}
    key, value = field.split(": ", 1)
    data[key] = value
    with pytest.raises(ValueError):
        check_metadata("\n".join(f"{k}: {v}" for k, v in data.items()).encode(), Version("0.1.0a1"))


def test_reject_empty_distribution_set(tmp_path):
    with pytest.raises(ValueError, match="No distribution"):
        check_artifacts(tmp_path, Version("0.1.0a1"), complete=True)


def test_reject_internal_doc_and_broken_link(tmp_path):
    readme = tmp_path / "README.md"
    readme.write_text("[missing](docs/missing.md)")
    with pytest.raises(AssertionError, match="Broken link"):
        check_docs(tmp_path)
    readme.write_text("See .internal/private.md")
    with pytest.raises(AssertionError, match="Internal material"):
        check_docs(tmp_path)
