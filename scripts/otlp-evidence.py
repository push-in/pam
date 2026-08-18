#!/usr/bin/env python3
"""Create or verify bounded PAM OTLP interoperability evidence."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import stat
import sys
from enum import IntEnum
from pathlib import Path


class EvidenceSuite(IntEnum):
    COLLECTOR_INTEROPERABILITY = 1


MAX_ARTIFACT_BYTES = 2 * 1024 * 1024
ARTIFACT_NAMES = ("collector.log", "metadata.json", "pam.stderr.log", "report.json")
MANIFEST_NAME = "evidence-manifest.json"


def describe(path: Path) -> dict[str, object]:
    metadata = path.lstat()
    if stat.S_ISLNK(metadata.st_mode) or not stat.S_ISREG(metadata.st_mode):
        raise ValueError(f"evidence artifact must be a regular file: {path.name}")
    if metadata.st_size > MAX_ARTIFACT_BYTES:
        raise ValueError(f"evidence artifact exceeds {MAX_ARTIFACT_BYTES} bytes: {path.name}")
    digest = hashlib.sha256(path.read_bytes()).hexdigest()
    return {"path": path.name, "bytes": metadata.st_size, "sha256": digest}


def artifacts(directory: Path) -> list[dict[str, object]]:
    missing = [name for name in ARTIFACT_NAMES if not (directory / name).exists()]
    if missing:
        raise ValueError(f"evidence artifacts are missing: {', '.join(missing)}")
    return [describe(directory / name) for name in ARTIFACT_NAMES]


def load_json(path: Path) -> dict[str, object]:
    value = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(value, dict):
        raise ValueError(f"expected a JSON object: {path.name}")
    return value


def create(directory: Path, suite: EvidenceSuite) -> dict[str, object]:
    metadata = load_json(directory / "metadata.json")
    report = load_json(directory / "report.json")
    if report.get("passed") is not True:
        raise ValueError("cannot publish failing OTLP evidence")
    source = metadata.get("source")
    if (
        isinstance(source, dict)
        and source.get("dirty") is True
        and os.environ.get("PAM_OTLP_ALLOW_DIRTY") != "1"
    ):
        raise ValueError("cannot publish OTLP evidence from a dirty worktree")
    manifest = {
        "schema_version": 1,
        "suite_id": int(suite),
        "source": metadata.get("source"),
        "collector": metadata.get("collector"),
        "protocol": metadata.get("protocol"),
        "gates": report.get("gates"),
        "artifacts": artifacts(directory),
    }
    manifest_path = directory / MANIFEST_NAME
    if manifest_path.is_symlink():
        raise ValueError("evidence manifest path must not be a symlink")
    manifest_path.write_text(
        json.dumps(manifest, indent=2, sort_keys=True) + "\n", encoding="utf-8"
    )
    return manifest


def verify(directory: Path, suite: EvidenceSuite) -> dict[str, object]:
    manifest_path = directory / MANIFEST_NAME
    if not manifest_path.is_file() or manifest_path.is_symlink():
        raise ValueError("evidence manifest is missing or unsafe")
    manifest = load_json(manifest_path)
    if manifest.get("schema_version") != 1 or manifest.get("suite_id") != int(suite):
        raise ValueError("evidence manifest schema or suite does not match")
    report = load_json(directory / "report.json")
    if report.get("passed") is not True or manifest.get("gates") != report.get("gates"):
        raise ValueError("OTLP evidence gates do not pass or match")
    source = manifest.get("source")
    if (
        isinstance(source, dict)
        and source.get("dirty") is True
        and os.environ.get("PAM_OTLP_ALLOW_DIRTY") != "1"
    ):
        raise ValueError("cannot verify publishable evidence from a dirty worktree")
    if manifest.get("artifacts") != artifacts(directory):
        raise ValueError("OTLP evidence artifacts do not match their manifest")
    return manifest


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("directory", type=Path)
    parser.add_argument("suite_id", type=int, choices=[int(EvidenceSuite.COLLECTOR_INTEROPERABILITY)])
    parser.add_argument("--verify", action="store_true")
    arguments = parser.parse_args()
    if arguments.directory.is_symlink():
        parser.error("evidence directory must not be a symlink")
    directory = arguments.directory.resolve()
    if not directory.is_dir():
        parser.error("evidence directory does not exist")
    suite = EvidenceSuite(arguments.suite_id)
    try:
        manifest = verify(directory, suite) if arguments.verify else create(directory, suite)
    except (OSError, ValueError, json.JSONDecodeError) as error:
        print(f"OTLP evidence error: {error}", file=sys.stderr)
        return 1
    action = "Verified" if arguments.verify else "Created"
    print(f"{action} OTLP evidence with {len(manifest['artifacts'])} artifacts.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
