#!/usr/bin/env bash
# RSS soak (method of nimbus-bench/results/2026-11-21_soak.md, scaled to a CI
# runner): serve benchmarks/soak/app.php with N workers, drive a mixed load
# with wrk until TARGET requests, sample VmRSS + VmSwap of every worker, and
# fail when a worker grows more than MAX_GROWTH_KIB after the warmup, when a
# worker was replaced, or when any response was not 2xx.
#
# Usage: scripts/soak.sh path/to/pam
# Env: TARGET (5000000), WORKERS (2), PHASE (30s), CONNECTIONS (32),
#      WARMUP_REQUESTS (1000000), MAX_GROWTH_KIB (4096), SAMPLE_SECONDS (10),
#      OUT (soak-out), SERVER_CPUS / LOAD_CPUS (taskset lists, optional).
set -euo pipefail

PAM_BIN=$(realpath "${1:?usage: scripts/soak.sh path/to/pam}")
ROOT=$(cd "$(dirname "$0")/.." && pwd)
TARGET=${TARGET:-5000000}
WORKERS=${WORKERS:-2}
PHASE=${PHASE:-30s}
CONNECTIONS=${CONNECTIONS:-32}
WARMUP_REQUESTS=${WARMUP_REQUESTS:-1000000}
MAX_GROWTH_KIB=${MAX_GROWTH_KIB:-4096}
SAMPLE_SECONDS=${SAMPLE_SECONDS:-10}
OUT=${OUT:-soak-out}
PORT=${PORT:-18093}
mkdir -p "$OUT"
: >"$OUT/rss.csv"
: >"$OUT/phases.csv"

pin() {
  local cpus=$1
  shift
  if [[ -n $cpus ]] && command -v taskset >/dev/null; then
    taskset -c "$cpus" "$@"
  else
    "$@"
  fi
}

SERVER=""
SAMPLER=""
cleanup() {
  set +e
  [[ -n $SAMPLER ]] && kill "$SAMPLER" 2>/dev/null
  if [[ -n $SERVER ]]; then
    kill -TERM -- "-$SERVER" 2>/dev/null
    sleep 2
    kill -KILL -- "-$SERVER" 2>/dev/null
  fi
}
trap cleanup EXIT INT TERM

PAM_SOAK_PORT=$PORT PAM_ENV=production setsid \
  ${SERVER_CPUS:+taskset -c "$SERVER_CPUS"} "$PAM_BIN" start "$ROOT/benchmarks/soak/app.php" \
  --workers "$WORKERS" --max-requests 1000000000 >"$OUT/server.log" 2>&1 &
SERVER=$!
for _ in $(seq 120); do
  curl -fsS -o /dev/null "http://127.0.0.1:$PORT/ping" 2>/dev/null && break
  sleep 0.5
done
curl -fsS -o /dev/null "http://127.0.0.1:$PORT/ping" || {
  cat "$OUT/server.log"
  echo "soak: the server did not start" >&2
  exit 1
}

workers() {
  # Workers are the leaf processes of the server's session (forked from the
  # master or from its zygote).
  local pid
  for pid in $(pgrep -s "$SERVER" || true); do
    [[ $pid == "$SERVER" ]] && continue
    pgrep -P "$pid" >/dev/null || echo "$pid"
  done | sort -n
}
sleep 2
INITIAL_WORKERS=$(workers | tr '\n' ' ')
echo "workers: $INITIAL_WORKERS"

START=$(date +%s)
TOTAL_FILE="$OUT/total"
echo 0 >"$TOTAL_FILE"
(
  while true; do
    now=$(($(date +%s) - START))
    total=$(cat "$TOTAL_FILE")
    for pid in $(workers); do
      kib=$(awk '/^VmRSS:/ {r=$2} /^VmSwap:/ {s=$2} END {if (r) print r + s}' "/proc/$pid/status" 2>/dev/null || true)
      [[ -n $kib ]] && echo "$now,$total,$pid,$kib" >>"$OUT/rss.csv"
    done
    sleep "$SAMPLE_SECONDS"
  done
) &
SAMPLER=$!

total=0
failures=0
phase=0
while ((total < TARGET)); do
  phase=$((phase + 1))
  pin "${LOAD_CPUS:-}" wrk -t2 -c"$CONNECTIONS" -d"$PHASE" -s "$ROOT/benchmarks/soak/mixed.lua" \
    "http://127.0.0.1:$PORT" >"$OUT/wrk-$phase.txt" 2>&1
  requests=$(awk '/requests in/ {print $1}' "$OUT/wrk-$phase.txt")
  non2xx=$(awk '/Non-2xx/ {print $NF}' "$OUT/wrk-$phase.txt")
  errors=$(awk '/Socket errors/ {gsub(",", ""); print $4 + $6 + $8 + $10}' "$OUT/wrk-$phase.txt")
  total=$((total + ${requests:-0}))
  failures=$((failures + ${non2xx:-0} + ${errors:-0}))
  echo "$total" >"$TOTAL_FILE"
  echo "$phase,$(($(date +%s) - START)),${requests:-0},${non2xx:-0},${errors:-0},$total" >>"$OUT/phases.csv"
  echo "phase $phase: ${requests:-0} requests (total $total), non-2xx ${non2xx:-0}, socket errors ${errors:-0}"
done
sleep "$SAMPLE_SECONDS"
kill "$SAMPLER" 2>/dev/null || true
SAMPLER=""
FINAL_WORKERS=$(workers | tr '\n' ' ')

python3 - "$OUT/rss.csv" "$WARMUP_REQUESTS" "$MAX_GROWTH_KIB" "$total" "$failures" \
  "$INITIAL_WORKERS" "$FINAL_WORKERS" "${GITHUB_STEP_SUMMARY:-/dev/null}" <<'PY'
import collections
import sys

path, warmup, limit, total, failures, initial, final, summary = sys.argv[1:]
warmup, limit, total, failures = int(warmup), int(limit), int(total), int(failures)
samples = collections.defaultdict(list)
for line in open(path):
    seconds, requests, pid, kib = (int(value) for value in line.strip().split(","))
    samples[pid].append((seconds, requests, kib))

problems = []
if failures:
    problems.append(f"{failures} non-2xx responses or socket errors")
if initial.split() != final.split():
    problems.append(f"workers changed during the soak: {initial.strip()} -> {final.strip()}")
lines = [
    "## RSS soak",
    "",
    f"{total:,} requests. Growth limit after {warmup:,} warmup requests: {limit} KiB per worker (RSS + swap).",
    "",
    "| Worker | After warmup (KiB) | Peak after warmup (KiB) | Final (KiB) | Growth (KiB) |",
    "| --- | --- | --- | --- | --- |",
]
for pid, points in sorted(samples.items()):
    settled = [kib for _, requests, kib in points if requests >= warmup]
    if len(settled) < 2:
        problems.append(f"worker {pid}: not enough samples after the warmup")
        continue
    growth = max(settled) - settled[0]
    lines.append(f"| {pid} | {settled[0]} | {max(settled)} | {settled[-1]} | {growth} |")
    if growth > limit:
        problems.append(f"worker {pid} grew {growth} KiB after the warmup (limit {limit})")
lines += ["", "Result: " + ("FAIL: " + "; ".join(problems) if problems else "plateau, no growth past the limit")]
report = "\n".join(lines) + "\n"
print(report)
with open(summary, "a") as handle:
    handle.write(report)
sys.exit(1 if problems else 0)
PY
