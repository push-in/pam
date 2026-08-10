#!/usr/bin/env python3
"""Validate repository-local Markdown links without network access."""

from __future__ import annotations

import re
import sys
from pathlib import Path
from urllib.parse import unquote


ROOT = Path(__file__).resolve().parent.parent
LINK = re.compile(r"(?<!!)\[[^\]]+\]\(([^)]+)\)")
failures: list[str] = []


for document in sorted((ROOT / "docs").rglob("*.md")):
    source = document.read_text(encoding="utf-8")
    for line_number, line in enumerate(source.splitlines(), 1):
        for raw_target in LINK.findall(line):
            target = raw_target.strip().strip("<>").split(maxsplit=1)[0]
            if not target or target.startswith(("#", "http://", "https://", "mailto:")):
                continue
            path_text = unquote(target.split("#", 1)[0])
            if not path_text:
                continue
            resolved = (document.parent / path_text).resolve()
            try:
                resolved.relative_to(ROOT.resolve())
            except ValueError:
                failures.append(f"{document.relative_to(ROOT)}:{line_number}: link escapes repository: {target}")
                continue
            if not resolved.exists():
                failures.append(f"{document.relative_to(ROOT)}:{line_number}: missing link target: {target}")


if failures:
    print("\n".join(failures), file=sys.stderr)
    raise SystemExit(1)

print("Documentation links are valid.")
