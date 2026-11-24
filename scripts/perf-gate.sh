#!/usr/bin/env bash
# Relative throughput gate: the same runner benchmarks the base build and the
# candidate build in alternating rounds (A B B A ...), so drift of the machine
# hits both alike, and compares the medians. Fails when the candidate's median
# is more than MAX_REGRESSION_PERCENT below the base's.
#
# Usage: scripts/perf-gate.sh path/to/base/pam path/to/candidate/pam
# Env: ROUNDS (6), DURATION (10s), CONNECTIONS (32), MAX_REGRESSION_PERCENT (5),
#      PATHS ("/ping /headers"), SERVER_CPUS / LOAD_CPUS (taskset lists).
set -euo pipefail

BASE=$(realpath "${1:?usage: scripts/perf-gate.sh base-pam candidate-pam}")
CANDIDATE=$(realpath "${2:?usage: scripts/perf-gate.sh base-pam candidate-pam}")
ROOT=$(cd "$(dirname "$0")/.." && pwd)
ROUNDS=${ROUNDS:-6}
DURATION=${DURATION:-10s}
CONNECTIONS=${CONNECTIONS:-32}
MAX_REGRESSION_PERCENT=${MAX_REGRESSION_PERCENT:-5}
PATHS=${PATHS:-/ping /headers}
PORT=${PORT:-18094}
OUT=${OUT:-perf-out}
mkdir -p "$OUT"
: >"$OUT/runs.csv"

SERVER=""
stop_server() {
  if [[ -n $SERVER ]]; then
    kill -TERM -- "-$SERVER" 2>/dev/null || true
    for _ in $(seq 50); do
      kill -0 "$SERVER" 2>/dev/null || break
      sleep 0.1
    done
    kill -KILL -- "-$SERVER" 2>/dev/null || true
    wait "$SERVER" 2>/dev/null || true
    SERVER=""
  fi
}
trap stop_server EXIT INT TERM

measure() {
  local label=$1 binary=$2 round=$3 path rps
  PAM_SOAK_PORT=$PORT PAM_ENV=production setsid \
    ${SERVER_CPUS:+taskset -c "$SERVER_CPUS"} "$binary" start "$ROOT/benchmarks/soak/app.php" \
    --workers 1 >"$OUT/server-$label-$round.log" 2>&1 &
  SERVER=$!
  for _ in $(seq 100); do
    curl -fsS -o /dev/null "http://127.0.0.1:$PORT/ping" 2>/dev/null && break
    sleep 0.1
  done
  for path in $PATHS; do
    # Warm the worker (opcache, allocator, connection pools) first.
    ${LOAD_CPUS:+taskset -c "$LOAD_CPUS"} wrk -t2 -c"$CONNECTIONS" -d2s "http://127.0.0.1:$PORT$path" >/dev/null 2>&1
    ${LOAD_CPUS:+taskset -c "$LOAD_CPUS"} wrk -t2 -c"$CONNECTIONS" -d"$DURATION" \
      "http://127.0.0.1:$PORT$path" >"$OUT/wrk-$label-$round-${path//\//_}.txt" 2>&1
    rps=$(awk '/^Requests\/sec:/ {print $2}' "$OUT/wrk-$label-$round-${path//\//_}.txt")
    if grep -q "Non-2xx" "$OUT/wrk-$label-$round-${path//\//_}.txt"; then
      echo "perf-gate: $label answered non-2xx on $path" >&2
      cat "$OUT/wrk-$label-$round-${path//\//_}.txt" >&2
      exit 1
    fi
    echo "$label,$round,$path,${rps:-0}" >>"$OUT/runs.csv"
    echo "round $round $label $path: ${rps:-0} req/s"
  done
  stop_server
}

for round in $(seq "$ROUNDS"); do
  # A B, then B A: neither build always runs first.
  if ((round % 2)); then
    measure base "$BASE" "$round"
    measure candidate "$CANDIDATE" "$round"
  else
    measure candidate "$CANDIDATE" "$round"
    measure base "$BASE" "$round"
  fi
done

python3 - "$OUT/runs.csv" "$MAX_REGRESSION_PERCENT" "${GITHUB_STEP_SUMMARY:-/dev/null}" <<'PY'
import collections
import statistics
import sys

path, limit, summary = sys.argv[1], float(sys.argv[2]), sys.argv[3]
runs = collections.defaultdict(list)
for line in open(path):
    label, _, route, rps = line.strip().split(",")
    runs[(route, label)].append(float(rps))

routes = sorted({route for route, _ in runs})
lines = [
    "## Throughput gate (candidate vs base, same runner)",
    "",
    f"Median of {len(runs[(routes[0], 'base')])} alternating rounds; fails below -{limit:g}%.",
    "",
    "| Route | Base req/s | Candidate req/s | Change | Base spread |",
    "| --- | --- | --- | --- | --- |",
]
failed = []
for route in routes:
    base = statistics.median(runs[(route, "base")])
    candidate = statistics.median(runs[(route, "candidate")])
    change = (candidate - base) / base * 100 if base else 0.0
    spread = (max(runs[(route, "base")]) - min(runs[(route, "base")])) / base * 100 if base else 0.0
    lines.append(f"| {route} | {base:,.0f} | {candidate:,.0f} | {change:+.1f}% | {spread:.1f}% |")
    if change < -limit:
        failed.append(f"{route} {change:+.1f}%")
lines += ["", "Result: " + (("REGRESSION: " + ", ".join(failed)) if failed else "within the limit")]
report = "\n".join(lines) + "\n"
print(report)
with open(summary, "a") as handle:
    handle.write(report)
sys.exit(1 if failed else 0)
PY
