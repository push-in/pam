#!/usr/bin/env python3
"""Markdown table (rps, p50, p99, memory) from bench results.csv."""
import csv
import sys

rows = list(csv.DictReader(open(sys.argv[1])))
servers = list(dict.fromkeys(r["server"] for r in rows))
endpoints = list(dict.fromkeys(r["endpoint"] for r in rows))
print("| Endpoint | " + " | ".join(servers) + " |")
print("|---|" + "---|" * len(servers))
for endpoint in endpoints:
    cells = []
    for server in servers:
        r = next((r for r in rows if r["server"] == server and r["endpoint"] == endpoint), None)
        cells.append("-" if r is None else f"{float(r['rps']):,.0f} rps · p50 {r['p50_ms']} ms · p99 {r['p99_ms']} ms"
                     + (f" · {r['errors']} err" if r["errors"] not in ("0", "") else ""))
    print(f"| {endpoint} | " + " | ".join(cells) + " |")
print("\n| Server | Memory after run (MiB) | Peak (MiB) |\n|---|---|---|")
for server in servers:
    mine = [r for r in rows if r["server"] == server]
    print(f"| {server} | {mine[-1]['memory_current_mb']} | {max(int(r['memory_peak_mb']) for r in mine)} |")
