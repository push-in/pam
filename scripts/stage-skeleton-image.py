#!/usr/bin/env python3
"""Stage the Nimbus skeleton as a self-contained Docker build context.

The image is built from the monorepo, so it carries the Nimbus framework of the
same commit even before that version reaches Packagist: Nimbus is copied into
`.packages/nimbus` and registered as a Composer path repository (copied, not
symlinked) with the version it is about to be released as. Every other package
still comes from Packagist.

    scripts/stage-skeleton-image.py build/skeleton-image
    docker build build/skeleton-image
"""

from __future__ import annotations

import json
import re
import shutil
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SKELETON = ROOT / "packages" / "skeleton"
NIMBUS = ROOT / "packages" / "nimbus"
NIMBUS_PARTS = ("src", "resources", "database", "bin", "composer.json", "LICENSE")


def nimbus_version() -> str:
    """The latest released version, or its next minor when Unreleased has entries."""
    changelog = (NIMBUS / "CHANGELOG.md").read_text(encoding="utf-8")
    released = re.search(r"^## (\d+)\.(\d+)\.(\d+)", changelog, re.MULTILINE)
    if released is None:
        raise SystemExit("packages/nimbus/CHANGELOG.md has no released version")
    major, minor, patch = (int(part) for part in released.groups())
    unreleased = re.search(r"^## Unreleased\s*$(.*?)^## ", changelog, re.MULTILINE | re.DOTALL)
    if unreleased is not None and unreleased.group(1).strip():
        return f"{major}.{minor + 1}.0"
    return f"{major}.{minor}.{patch}"


def main() -> int:
    if len(sys.argv) != 2:
        print("usage: stage-skeleton-image.py OUTPUT_DIRECTORY", file=sys.stderr)
        return 2
    output = Path(sys.argv[1]).resolve()
    if output.exists():
        shutil.rmtree(output)
    shutil.copytree(
        SKELETON,
        output,
        ignore=shutil.ignore_patterns("vendor", "var", ".env", ".pam", "composer.lock"),
    )
    target = output / ".packages" / "nimbus"
    target.mkdir(parents=True)
    for part in NIMBUS_PARTS:
        source = NIMBUS / part
        if source.is_dir():
            shutil.copytree(source, target / part)
        elif source.exists():
            shutil.copy2(source, target / part)

    version = nimbus_version()
    manifest_path = output / "composer.json"
    manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    manifest["repositories"] = [
        {
            "type": "path",
            "url": ".packages/nimbus",
            "options": {"symlink": False, "versions": {"pushinbr/nimbus": version}},
        },
        *manifest.get("repositories", []),
    ]
    manifest_path.write_text(json.dumps(manifest, indent=4) + "\n", encoding="utf-8")
    print(f"Staged {output} with pushinbr/nimbus {version} from {NIMBUS.relative_to(ROOT)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
