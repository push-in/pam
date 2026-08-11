#!/usr/bin/env bash

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
RESULTS="${PAM_BENCH_RESULTS:-$ROOT/benchmarks/octane/results}"
PORT="${PAM_BENCH_PORT:-38080}"
THREADS="${PAM_BENCH_THREADS:-4}"
CONNECTIONS="${PAM_BENCH_CONNECTIONS:-128}"
DURATION="${PAM_BENCH_DURATION:-15s}"
WARMUP_DURATION="${PAM_BENCH_WARMUP_DURATION:-5s}"
ROUNDS="${PAM_BENCH_ROUNDS:-3}"
COOLDOWN="${PAM_BENCH_COOLDOWN:-2}"
WORKERS="${PAM_BENCH_WORKERS:-1}"
LOGICAL_CPUS="$(getconf _NPROCESSORS_ONLN)"
CPU_SPLIT=$((LOGICAL_CPUS / 2))
SERVER_CPUSET="${PAM_BENCH_SERVER_CPUSET:-0-$((CPU_SPLIT - 1))}"
LOAD_CPUSET="${PAM_BENCH_LOAD_CPUSET:-$CPU_SPLIT-$((LOGICAL_CPUS - 1))}"
export PAM_BENCH_SERVER_CPUSET="$SERVER_CPUSET"
export PAM_BENCH_LOAD_CPUSET="$LOAD_CPUSET"
FRANKEN_THREADS=$((WORKERS + 1))
PAM_BINARY="${PAM_BENCH_BINARY:-$ROOT/target/release/pam}"
export PAM_BENCH_BINARY="$PAM_BINARY"
FRANKEN_IMAGE="dunglas/frankenphp@sha256:c27a8112bba186ceb309030d356a3474fba1fe66a1855d665cf5e447adf58eaf"
export PAM_BENCH_FRANKEN_IMAGE="$FRANKEN_IMAGE"
SWOOLE_IMAGE="pam-bench-openswoole:25.2-php8.4.24"
export PAM_BENCH_SWOOLE_IMAGE="$SWOOLE_IMAGE"
RUNTIME_ORDER="${PAM_BENCH_RUNTIME_ORDER:-pam frankenphp openswoole}"
export PAM_BENCH_RUNTIME_ORDER="$RUNTIME_ORDER"
SERVER_PID=""
SAMPLER_PID=""

cleanup() {
    stop_sampler
    if [[ -n "$SERVER_PID" ]]; then
        kill -TERM "$SERVER_PID" 2>/dev/null || true
        for _ in $(seq 1 100); do
            kill -0 "$SERVER_PID" 2>/dev/null || break
            sleep 0.05
        done
        kill -KILL "$SERVER_PID" 2>/dev/null || true
        wait "$SERVER_PID" 2>/dev/null || true
    fi
    docker rm -f pam-bench-franken pam-bench-swoole >/dev/null 2>&1 || true
}
trap cleanup EXIT INT TERM

start_sampler() {
    local runtime="$1"
    local root_pid="$2"
    if [[ "${PAM_BENCH_SAMPLE_RESOURCES:-1}" != "1" ]]; then
        return 0
    fi
    "$ROOT/benchmarks/octane/sample-process.sh" \
        "$root_pid" "$RESULTS/resources.$runtime.csv" &
    SAMPLER_PID=$!
}

stop_sampler() {
    if [[ -n "$SAMPLER_PID" ]]; then
        kill -TERM "$SAMPLER_PID" 2>/dev/null || true
        wait "$SAMPLER_PID" 2>/dev/null || true
        SAMPLER_PID=""
    fi
}

wait_ready() {
    local runtime="$1"
    local deadline=$((SECONDS + 30))
    until curl --fail --silent --connect-timeout 1 --max-time 1 \
        "http://127.0.0.1:$PORT/api/ping" >/dev/null; do
        if (( SECONDS >= deadline )); then
            echo "benchmark server did not become ready" >&2
            exit 1
        fi
        sleep 0.05
    done
    local identity
    case "$runtime" in
        pam)
            identity="$(curl --fail --silent --connect-timeout 1 --max-time 2 \
                "http://127.0.0.1:$PORT/metrics")"
            [[ "$identity" == *"pam_http_requests_total"* ]] || {
                echo "port $PORT is not served by PAM" >&2
                exit 1
            }
            ;;
        frankenphp)
            identity="$(curl --silent --show-error --connect-timeout 1 --max-time 2 \
                --head "http://127.0.0.1:$PORT/api/ping")"
            [[ "${identity,,}" == *"server: frankenphp"* ]] || {
                echo "port $PORT is not served by FrankenPHP" >&2
                exit 1
            }
            ;;
        openswoole)
            docker inspect --format '{{.State.Running}}' pam-bench-swoole | grep -qx true || {
                echo "OpenSwoole container exited before readiness" >&2
                exit 1
            }
            ;;
    esac
}

capture_runtime_identity() {
    local runtime="$1"
    curl --fail --silent --show-error --connect-timeout 1 --max-time 2 \
        "http://127.0.0.1:$PORT/api/runtime" >"$RESULTS/runtime.$runtime.json"
    php -r '
        $runtime = $argv[1];
        $data = json_decode(file_get_contents($argv[2]), true, flags: JSON_THROW_ON_ERROR);
        foreach (["php_version", "zts", "debug", "opcache", "jit_enabled", "sapi"] as $field) {
            if (!array_key_exists($field, $data)) throw new RuntimeException("$runtime identity misses $field");
        }
        if ($data["opcache"] !== true || $data["jit_enabled"] !== true) {
            throw new RuntimeException("$runtime must benchmark with OPcache and JIT enabled");
        }
    ' "$runtime" "$RESULTS/runtime.$runtime.json"
}

stop_local_server() {
    kill -TERM "$SERVER_PID" 2>/dev/null || true
    for _ in $(seq 1 100); do
        if ! kill -0 "$SERVER_PID" 2>/dev/null; then
            wait "$SERVER_PID" 2>/dev/null || true
            SERVER_PID=""
            return
        fi
        sleep 0.05
    done
    kill -KILL "$SERVER_PID" 2>/dev/null || true
    wait "$SERVER_PID" 2>/dev/null || true
    SERVER_PID=""
}

measure_round() {
    local runtime="$1"
    local path="$2"
    local round="$3"
    taskset -c "$LOAD_CPUSET" wrk -t"$THREADS" -c"$CONNECTIONS" -d"$WARMUP_DURATION" \
        "http://127.0.0.1:$PORT$path" >/dev/null
    local output="$RESULTS/$runtime.round-$round.txt"
    taskset -c "$LOAD_CPUSET" wrk -t"$THREADS" -c"$CONNECTIONS" -d"$DURATION" --latency \
        "http://127.0.0.1:$PORT$path" | tee "$output"
    php "$ROOT/benchmarks/octane/parse-wrk.php" "$runtime" "$round" "$output"
    sleep "$COOLDOWN"
}

measure_suite() {
    local runtime="$1"
    local edge_name="$2"
    local scenarios=(uncached blade database large-json edge)
    local paths=(/api/ping /api/blade /api/database /api/large-json /api/cached)
    for round in $(seq 1 "$ROUNDS"); do
        local offset=$(( (round - 1 + WORKERS) % ${#scenarios[@]} ))
        for position in $(seq 0 $((${#scenarios[@]} - 1))); do
            local index=$(( (position + offset) % ${#scenarios[@]} ))
            local scenario="${scenarios[$index]}"
            local name="$runtime-$scenario"
            [[ "$scenario" == "edge" ]] && name="$runtime-$edge_name"
            measure_round "$name" "${paths[$index]}" "$round"
        done
    done
}

benchmark_pam() {
    echo "Benchmarking PAM"
    (
        cd "$ROOT/packages/octane/tests/Fixtures/laravel"
        exec taskset -c "$SERVER_CPUSET" env PAM_RESPONSE_CACHE_PATHS=/api/cached \
            # The opt-in cache can exceed ten million requests during the
            # release matrix. Recycling here would benchmark supervisor churn
            # instead of the HTTP/cache implementation, so keep the release
            # evidence below a deliberately unreachable per-run ceiling.
            "$PAM_BINARY" start artisan --workers "$WORKERS" --max-requests 1000000000 \
                -- pam:octane --host=127.0.0.1 --port="$PORT"
    ) >"$RESULTS/pam.server.log" 2>&1 &
    SERVER_PID=$!
    wait_ready pam
    capture_runtime_identity pam
    start_sampler pam "$SERVER_PID"
    measure_suite pam edge-cache
    stop_sampler
    stop_local_server
}

benchmark_frankenphp() {
    echo "Benchmarking FrankenPHP"
    docker run --rm --name pam-bench-franken --network host --cpuset-cpus "$SERVER_CPUSET" \
        -v "$ROOT:/workspace" -w /workspace \
        -e APP_ENV=production -e APP_DEBUG=false \
        -e COMPOSER_VENDOR_DIR=/workspace/packages/octane/vendor \
        -e PAM_BENCH_PORT="$PORT" -e PAM_BENCH_WORKERS="$WORKERS" \
        -e PAM_BENCH_FRANKEN_THREADS="$FRANKEN_THREADS" \
        -v "$ROOT/benchmarks/octane/Caddyfile:/etc/caddy/Caddyfile:ro" \
        "$FRANKEN_IMAGE" frankenphp run --config /etc/caddy/Caddyfile \
        >"$RESULTS/frankenphp.server.log" 2>&1 &
    SERVER_PID=$!
    wait_ready frankenphp
    capture_runtime_identity frankenphp
    start_sampler frankenphp "container:pam-bench-franken"
    measure_suite frankenphp edge-comparison
    stop_sampler
    docker rm -f pam-bench-franken >/dev/null 2>&1 || true
    wait "$SERVER_PID" || true
    SERVER_PID=""
}

benchmark_openswoole() {
    echo "Benchmarking OpenSwoole"
    docker run --rm --name pam-bench-swoole --network host --cpuset-cpus "$SERVER_CPUSET" \
        -v "$ROOT:/workspace" -w /workspace/packages/octane/tests/Fixtures/laravel \
        -v "$ROOT/benchmarks/octane/opcache.ini:/usr/local/etc/php/conf.d/99-pam-benchmark.ini:ro" \
        -e APP_ENV=production -e APP_DEBUG=false \
        -e COMPOSER_VENDOR_DIR=/workspace/packages/octane/vendor \
        "$SWOOLE_IMAGE" php artisan octane:start --server=swoole \
            --host=127.0.0.1 --port="$PORT" \
            --workers="$WORKERS" --max-requests=1000000 \
        >"$RESULTS/openswoole.server.log" 2>&1 &
    SERVER_PID=$!
    wait_ready openswoole
    capture_runtime_identity openswoole
    start_sampler openswoole "container:pam-bench-swoole"
    measure_suite openswoole edge-comparison
    stop_sampler
    docker rm -f pam-bench-swoole >/dev/null 2>&1 || true
    wait "$SERVER_PID" || true
    SERVER_PID=""
}

mkdir -p "$RESULTS"
find "$RESULTS" -maxdepth 1 -type f -delete

if [[ ! -x "$PAM_BINARY" ]]; then
    cargo build --release --manifest-path "$ROOT/Cargo.toml"
fi
"$PAM_BINARY" "$ROOT/benchmarks/octane/prepare-fixture.php"
php "$ROOT/benchmarks/octane/metadata.php" "$RESULTS"

if ! docker image inspect "$SWOOLE_IMAGE" >/dev/null 2>&1; then
    docker build --file "$ROOT/benchmarks/octane/Dockerfile.openswoole" \
        --tag "$SWOOLE_IMAGE" "$ROOT/benchmarks/octane"
fi

read -r -a runtimes <<<"$RUNTIME_ORDER"
[[ " ${runtimes[*]} " == *" pam "* && " ${runtimes[*]} " == *" frankenphp "* \
    && " ${runtimes[*]} " == *" openswoole "* && "${#runtimes[@]}" == 3 ]] || {
    echo "PAM_BENCH_RUNTIME_ORDER must contain pam, frankenphp and openswoole exactly once" >&2
    exit 64
}
for runtime in "${runtimes[@]}"; do
    "benchmark_$runtime"
done

php "$ROOT/benchmarks/octane/aggregate.php" "$RESULTS"
php "$ROOT/benchmarks/octane/resource-report.php" "$RESULTS"
