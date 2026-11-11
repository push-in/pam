#!/usr/bin/env python3
"""Per-test pass/fail table from the two JUnit runs of stage 1."""
import json
import os
import sys
import xml.etree.ElementTree as ET

out = sys.argv[1]
SWOOLE_HINTS = ("Swoole", "swoole", "Table", "TaskDispatcher", "Tick", "OctaneStore")
SWOOLE_ONLY = ("TableTest::", "OctaneStoreTest::", "SwooleRequestTest::")


def cases(path):
    result = {}
    if not os.path.exists(path):
        return result
    for case in ET.parse(path).getroot().iter("testcase"):
        name = f"{case.get('class', '').split(chr(92))[-1]}::{case.get('name')}"
        if case.find("failure") is not None or case.find("error") is not None:
            status = "FAIL"
        elif case.find("skipped") is not None:
            status = "SKIP"
        else:
            status = "PASS"
        result[name] = status
    return result


upstream = cases(os.path.join(out, "upstream.xml"))
pam = cases(os.path.join(out, "pam-driver.xml"))
cross = {}
if os.path.exists(os.path.join(out, "pam-crosscheck.json")):
    cross = json.load(open(os.path.join(out, "pam-crosscheck.json")))
rows = []
for name in sorted(set(upstream) | set(pam)):
    scope = "swoole-only" if any(h in name for h in SWOOLE_HINTS) else "all servers"
    rows.append({"test": name, "upstream": upstream.get(name, "-"), "pam_driver": pam.get(name, "-"), "scope": scope})


def count(column, status):
    return sum(1 for row in rows if row[column] == status)


# The tests Octane skips without the Swoole extension, and how many run on PAM.
swoole_only = [row for row in rows if row["test"].startswith(SWOOLE_ONLY)]


summary = {
    "swoole_only_tests": {
        "total": len(swoole_only),
        "upstream": {s: sum(1 for row in swoole_only if row["upstream"] == s) for s in ("PASS", "FAIL", "SKIP")},
    },
    "octane_tag": open(os.path.join(out, "octane-tag.txt")).read().strip() if os.path.exists(os.path.join(out, "octane-tag.txt")) else None,
    "tests": len(rows),
    "upstream": {s: count("upstream", s) for s in ("PASS", "FAIL", "SKIP")},
    "pam_driver": {s: count("pam_driver", s) for s in ("PASS", "FAIL", "SKIP")},
    "crosscheck_exchanges": cross.get("exchanges"),
    "crosscheck_mismatches": cross.get("mismatches", []),
    "rows": rows,
}
json.dump(summary, open(os.path.join(out, "summary.json"), "w"), indent=2)
with open(os.path.join(out, "summary.md"), "w") as md:
    md.write("| Test | Upstream suite (PAM PHP) | Through PAM client | Scope |\n|---|---|---|---|\n")
    for row in rows:
        md.write(f"| `{row['test']}` | {row['upstream']} | {row['pam_driver']} | {row['scope']} |\n")
print(json.dumps({k: v for k, v in summary.items() if k != "rows"}, indent=2))
sys.exit(1 if summary["upstream"]["FAIL"] or summary["pam_driver"]["FAIL"] or summary["crosscheck_mismatches"] else 0)
