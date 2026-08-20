#!/usr/bin/env python3
"""Select the newest stable release older than a candidate tag."""

from __future__ import annotations

import json
import re
import sys
from pathlib import Path


MAX_RELEASE_METADATA_BYTES = 1_048_576
SEMVER = re.compile(r"v?(\d+)\.(\d+)\.(\d+)")


def version(tag: object) -> tuple[int, int, int] | None:
    if not isinstance(tag, str):
        return None
    match = SEMVER.fullmatch(tag)
    return tuple(map(int, match.groups())) if match else None


def select_baseline(releases: object, candidate_tag: str) -> str:
    candidate = version(candidate_tag)
    if candidate is None:
        raise ValueError(f"candidate is not a stable SemVer tag: {candidate_tag}")
    if not isinstance(releases, list):
        raise ValueError("release metadata must be a JSON array")

    eligible: list[tuple[tuple[int, int, int], str]] = []
    for release in releases:
        if not isinstance(release, dict):
            continue
        tag = release.get("tagName")
        parsed = version(tag)
        if parsed is not None and parsed < candidate:
            eligible.append((parsed, tag))
    if not eligible:
        raise ValueError("no older stable release is available as a baseline")
    return max(eligible)[1]


def main() -> int:
    if len(sys.argv) != 3:
        print("usage: select-release-baseline.py RELEASES.json CANDIDATE_TAG", file=sys.stderr)
        return 64
    path = Path(sys.argv[1])
    if not path.is_file() or path.is_symlink():
        print("release metadata must be a regular file", file=sys.stderr)
        return 66
    if path.stat().st_size > MAX_RELEASE_METADATA_BYTES:
        print("release metadata exceeds 1 MiB", file=sys.stderr)
        return 65
    try:
        releases = json.loads(path.read_text(encoding="utf-8"))
        print(select_baseline(releases, sys.argv[2]))
    except (OSError, UnicodeError, json.JSONDecodeError, ValueError) as error:
        print(error, file=sys.stderr)
        return 65
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
