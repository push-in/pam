#!/usr/bin/env bash

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
RESULTS="${PAM_SOAK_RESULTS:-$ROOT/benchmarks/octane/results/soak}"
PORT="${PAM_SOAK_PORT:-38090}"
WORKERS="${PAM_SOAK_WORKERS:-4}"
CONNECTIONS="${PAM_SOAK_CONNECTIONS:-256}"
THREADS="${PAM_SOAK_THREADS:-4}"
DURATION="${PAM_SOAK_DURATION:-30m}"
PAM_BINARY="${PAM_SOAK_BINARY:-$ROOT/target/release/pam}"
SERVER_PID=""
SAMPLER_PID=""

cleanup() {
    [[ -z "$SAMPLER_PID" ]] || kill -TERM "$SAMPLER_PID" 2>/dev/null || true
    [[ -z "$SAMPLER_PID" ]] || wait "$SAMPLER_PID" 2>/dev/null || true
    if [[ -n "$SERVER_PID" ]]; then
        kill -TERM "$SERVER_PID" 2>/dev/null || true
        wait "$SERVER_PID" 2>/dev/null || true
    fi
}
trap cleanup EXIT INT TERM

mkdir -p "$RESULTS"
find "$RESULTS" -maxdepth 1 -type f -delete
[[ -x "$PAM_BINARY" ]] || cargo build --release --manifest-path "$ROOT/Cargo.toml"
"$PAM_BINARY" "$ROOT/benchmarks/octane/prepare-fixture.php"

(
    cd "$ROOT/packages/octane/tests/Fixtures/laravel"
    exec "$PAM_BINARY" start artisan --workers "$WORKERS" --max-requests 10000000 -- \
        pam:octane --host=127.0.0.1 --port="$PORT"
) >"$RESULTS/server.log" 2>&1 &
SERVER_PID=$!

deadline=$((SECONDS + 30))
until curl --fail --silent --connect-timeout 1 --max-time 1 \
    "http://127.0.0.1:$PORT/api/ping" >/dev/null; do
    ((SECONDS < deadline)) || { echo "soak server did not become ready" >&2; exit 1; }
    sleep 0.05
done

"$ROOT/benchmarks/octane/sample-process.sh" "$SERVER_PID" "$RESULTS/resources.pam.csv" &
SAMPLER_PID=$!
wrk -t"$THREADS" -c"$CONNECTIONS" -d5s "http://127.0.0.1:$PORT/api/ping" >/dev/null
wrk -t"$THREADS" -c"$CONNECTIONS" -d"$DURATION" --latency \
    "http://127.0.0.1:$PORT/api/ping" | tee "$RESULTS/soak.txt"
kill -TERM "$SAMPLER_PID" 2>/dev/null || true
wait "$SAMPLER_PID" 2>/dev/null || true
SAMPLER_PID=""

php "$ROOT/benchmarks/octane/parse-wrk.php" pam-soak 1 "$RESULTS/soak.txt"
PAM_BENCH_WORKERS="$WORKERS" PAM_BENCH_THREADS="$THREADS" \
    PAM_BENCH_CONNECTIONS="$CONNECTIONS" PAM_BENCH_DURATION="$DURATION" \
    PAM_BENCH_WARMUP_DURATION=5s PAM_BENCH_ROUNDS=1 \
    PAM_BENCH_RUNTIME_ORDER=pam PAM_BENCH_BINARY="$PAM_BINARY" \
    PAM_SOAK_MAX_RSS_GROWTH_BYTES="${PAM_SOAK_MAX_RSS_GROWTH_BYTES:-67108864}" \
    php "$ROOT/benchmarks/octane/metadata.php" "$RESULTS"
php "$ROOT/benchmarks/octane/soak-report.php" "$RESULTS"
php "$ROOT/benchmarks/octane/evidence-manifest.php" "$RESULTS" 3
php "$ROOT/benchmarks/octane/evidence-manifest.php" "$RESULTS" 3 --verify
