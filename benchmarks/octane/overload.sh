#!/usr/bin/env bash

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
RESULTS="${PAM_OVERLOAD_RESULTS:-$ROOT/benchmarks/octane/results/overload}"
PORT="${PAM_OVERLOAD_PORT:-38100}"
CONCURRENCY="${PAM_OVERLOAD_CONCURRENCY:-32}"
QUEUE_CAPACITY="${PAM_OVERLOAD_QUEUE_CAPACITY:-1}"
PAM_BINARY="${PAM_OVERLOAD_BINARY:-$ROOT/target/release/pam}"
SERVER_PID=""
WORK=""

cleanup() {
    [[ -z "$SERVER_PID" ]] || kill -TERM "$SERVER_PID" 2>/dev/null || true
    [[ -z "$SERVER_PID" ]] || wait "$SERVER_PID" 2>/dev/null || true
    [[ -z "$WORK" ]] || rm -rf -- "$WORK"
}
trap cleanup EXIT INT TERM

[[ "$CONCURRENCY" =~ ^[1-9][0-9]*$ && "$CONCURRENCY" -ge 3 ]] || {
    echo "PAM_OVERLOAD_CONCURRENCY must be an integer of at least 3" >&2; exit 64;
}
[[ "$QUEUE_CAPACITY" =~ ^[1-9][0-9]*$ ]] || {
    echo "PAM_OVERLOAD_QUEUE_CAPACITY must be a positive integer" >&2; exit 64;
}
mkdir -p "$RESULTS"
find "$RESULTS" -maxdepth 1 -type f -delete
WORK="$(mktemp -d)"
[[ -x "$PAM_BINARY" ]] || cargo build --locked --release --manifest-path "$ROOT/Cargo.toml"
"$PAM_BINARY" "$ROOT/benchmarks/octane/prepare-fixture.php"

(
    cd "$ROOT/packages/octane/tests/Fixtures/laravel"
    exec env PAM_PHP_QUEUE_CAPACITY="$QUEUE_CAPACITY" \
        "$PAM_BINARY" start artisan --workers 1 --max-requests 10000000 -- \
        pam:octane --host=127.0.0.1 --port="$PORT"
) >"$RESULTS/server.log" 2>&1 &
SERVER_PID=$!

deadline=$((SECONDS + 30))
until curl --fail --silent --connect-timeout 1 --max-time 1 \
    "http://127.0.0.1:$PORT/api/ping" >/dev/null; do
    kill -0 "$SERVER_PID" 2>/dev/null || {
        echo "overload server exited before readiness; inspect $RESULTS/server.log" >&2
        exit 1
    }
    ((SECONDS < deadline)) || { echo "overload server did not become ready" >&2; exit 1; }
    sleep 0.05
done

for request_id in $(seq 1 "$CONCURRENCY"); do
    (
        headers="$WORK/headers.$request_id"
        status_and_time="$(curl --silent --output /dev/null --dump-header "$headers" \
            --connect-timeout 1 --max-time 5 --write-out $'%{http_code}\t%{time_total}' \
            "http://127.0.0.1:$PORT/api/slow")"
        retry_after="$(sed -n 's/^[Rr]etry-[Aa]fter:[[:space:]]*\([^[:space:]\r]*\).*/\1/p' "$headers" | head -1)"
        printf '%s\t%s\t%s\n' "${status_and_time%%$'\t'*}" "$retry_after" "${status_and_time#*$'\t'}" \
            >"$WORK/sample.$request_id"
    ) &
done
wait
cat "$WORK"/sample.* >"$RESULTS/samples.tsv"
recovery_status="$(curl --silent --output /dev/null --connect-timeout 1 --max-time 2 \
    --write-out '%{http_code}' "http://127.0.0.1:$PORT/api/ping")"

PAM_BENCH_WORKERS=1 PAM_BENCH_CONNECTIONS="$CONCURRENCY" \
    PAM_OVERLOAD_CONCURRENCY="$CONCURRENCY" PAM_OVERLOAD_QUEUE_CAPACITY="$QUEUE_CAPACITY" \
    PAM_BENCH_DURATION=single-burst PAM_BENCH_WARMUP_DURATION=none \
    PAM_BENCH_ROUNDS=1 PAM_BENCH_RUNTIME_ORDER=pam \
    PAM_BENCH_BINARY="$PAM_BINARY" php "$ROOT/benchmarks/octane/metadata.php" "$RESULTS"
php "$ROOT/benchmarks/octane/overload-report.php" \
    "$RESULTS/samples.tsv" "$RESULTS/overload-report.json" "$recovery_status"
php "$ROOT/benchmarks/octane/evidence-manifest.php" "$RESULTS" 4
php "$ROOT/benchmarks/octane/evidence-manifest.php" "$RESULTS" 4 --verify
