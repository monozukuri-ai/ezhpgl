"""Check that public Markdown links resolve and contain no internal work notes."""

from __future__ import annotations

import re
from pathlib import Path
from urllib.parse import unquote, urlsplit


def check(root: Path) -> tuple[int, int]:
    documents = [root / "README.md", *sorted((root / "docs").glob("*.md"))]
    forbidden = re.compile(r"\.internal(?:/|\b)|/home/|\bM[0-9]\b|sibling.checkout|m6-", re.IGNORECASE)
    links = 0
    for path in documents:
        text = path.read_text(encoding="utf-8")
        assert not forbidden.search(text), f"Internal material in {path}"
        for match in re.finditer(r"\[[^\]\n]*\]\(([^\s)]+)\)", text):
            target = urlsplit(match[1])
            if target.scheme or not target.path:
                continue
            resolved = (path.parent / unquote(target.path)).resolve()
            assert resolved.is_relative_to(root.resolve()), (path, match[1])
            assert resolved.exists(), f"Broken link: {path}: {match[1]}"
            links += 1
    return len(documents), links


if __name__ == "__main__":
    count, links = check(Path(__file__).resolve().parents[1])
    print(f"PASS {count} public documents, {links} local links")
