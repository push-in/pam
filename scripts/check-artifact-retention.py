#!/usr/bin/env python3
"""Require an explicit bounded lifetime for every GitHub Actions artifact."""

from pathlib import Path
import re
import sys


ROOT = Path(__file__).resolve().parent.parent
UPLOAD = re.compile(r"^(?P<indent>\s*)-?\s*uses:\s*actions/upload-artifact@")
STEP = re.compile(r"^(?P<indent>\s*)-\s+(?:name:|uses:)")
RETENTION = re.compile(r"^\s*retention-days:\s*(?P<days>\d+)\s*(?:#.*)?$")
NAME = re.compile(r"^\s*name:\s*(?P<name>.+?)\s*$")


def artifact_steps(path: Path) -> list[tuple[int, list[str]]]:
    lines = path.read_text(encoding="utf-8").splitlines()
    results: list[tuple[int, list[str]]] = []
    for index, line in enumerate(lines):
        match = UPLOAD.match(line)
        if match is None:
            continue
        indent = len(match.group("indent"))
        end = len(lines)
        for candidate in range(index + 1, len(lines)):
            boundary = STEP.match(lines[candidate])
            if boundary is not None and len(boundary.group("indent")) <= indent:
                end = candidate
                break
        results.append((index + 1, lines[index:end]))
    return results


def main() -> int:
    failures: list[str] = []
    uploads = 0
    for path in sorted((ROOT / ".github" / "workflows").glob("*.yml")):
        for line, step in artifact_steps(path):
            uploads += 1
            retention = next((RETENTION.match(value) for value in step if RETENTION.match(value)), None)
            if retention is None:
                failures.append(f"{path.relative_to(ROOT)}:{line}: missing retention-days")
                continue
            days = int(retention.group("days"))
            if not 1 <= days <= 30:
                failures.append(
                    f"{path.relative_to(ROOT)}:{line}: retention-days must be between 1 and 30"
                )
            artifact_name = next(
                (match.group("name") for value in step if (match := NAME.match(value))),
                "",
            )
            if "prerequisite" in artifact_name.lower() and days != 1:
                failures.append(
                    f"{path.relative_to(ROOT)}:{line}: transient prerequisites must retain for 1 day"
                )
    if uploads == 0:
        failures.append("no upload-artifact steps found")
    if failures:
        print("\n".join(failures), file=sys.stderr)
        return 1
    print(f"Artifact retention policy passed for {uploads} uploads.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
