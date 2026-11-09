#!/usr/bin/env python3
"""Aggregate every stage's results into one Markdown summary."""
import json
import os
import sys

out = sys.argv[1]
print("# PAM Octane readiness results\n")
suite = os.path.join(out, "octane-suite", "summary.json")
if os.path.exists(suite):
    s = json.load(open(suite))
    print(f"## laravel/octane {s['octane_tag']} test suite\n")
    print(f"- Upstream suite on PAM's PHP: {s['upstream']}")
    print(f"- Through the PAM client: {s['pam_driver']}, {s['crosscheck_exchanges']} exchanges cross-checked, "
          f"{len(s['crosscheck_mismatches'])} mismatches\n")
for name in sorted(os.listdir(out)):
    if name.startswith("matrix-") and name.endswith(".json"):
        print(f"## Feature matrix ({name[7:-5]})\n\n| Feature | Status | Detail |\n|---|---|---|")
        for row in json.load(open(os.path.join(out, name))):
            print(f"| {row['feature']} | {row['status']} | {row['detail'].replace('|', '/')} |")
        print()
for stage in ("ecosystem", "bench"):
    path = os.path.join(out, stage, "summary.md" if stage == "ecosystem" else "results.md")
    if os.path.exists(path):
        print(f"## {stage.capitalize()}\n")
        print(open(path).read())
