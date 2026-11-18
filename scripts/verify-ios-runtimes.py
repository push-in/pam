#!/usr/bin/env python3
"""Checks runtime/ios-runtimes.json: one entry per catalog release whose
published asset downloads and matches its pinned SHA-256 (the iOS runtime
pam-native installs on a fresh Mac). Missing entries are reported, and fail
with --require-all."""

import hashlib
import json
import sys
import urllib.request
from pathlib import Path

root = Path(__file__).resolve().parent.parent
catalog = json.loads((root / "runtime/catalog.json").read_text())
manifest = json.loads((root / "runtime/ios-runtimes.json").read_text())
runtimes = manifest.get("runtimes", {})
failed = False
for runtime_id in catalog["releases"]:
    entry = runtimes.get(runtime_id)
    if entry is None:
        print(f"{runtime_id}: no published iOS runtime")
        failed |= "--require-all" in sys.argv
        continue
    url, expected = entry["url"], entry["sha256"].lower()
    if not url.startswith("https://") or len(expected) != 64:
        print(f"{runtime_id}: invalid entry {entry}")
        failed = True
        continue
    digest = hashlib.sha256()
    with urllib.request.urlopen(url, timeout=120) as response:  # noqa: S310 (https only)
        for block in iter(lambda: response.read(1 << 20), b""):
            digest.update(block)
    actual = digest.hexdigest()
    ok = actual == expected
    print(f"{runtime_id}: {'ok' if ok else f'checksum {actual} != {expected}'} {url}")
    failed |= not ok
for runtime_id in runtimes:
    if runtime_id not in catalog["releases"]:
        print(f"{runtime_id}: not in runtime/catalog.json")
        failed = True
sys.exit(1 if failed else 0)
