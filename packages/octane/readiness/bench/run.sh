#!/usr/bin/env bash
# Stage 4: reproducible benchmark of Laravel Octane servers on one Laravel app.
# PAM Octane vs FrankenPHP vs Swoole vs RoadRunner, same app, same CPU set,
# same worker count, sequential runs, wrk from a disjoint CPU set.
source "$(dirname "$0")/../lib.sh"
BENCH_DIR="$READINESS_DIR/bench"
set -a; . "$BENCH_DIR/versions.env"; set +a
APP="$WORK_DIR/bench-app"
OUT="$RESULTS_DIR/bench"
PORT="${BENCH_PORT:-8400}"
SERVERS="${BENCH_SERVERS:-pam frankenphp swoole roadrunner}"
ENDPOINTS="${BENCH_ENDPOINTS:-hello json db view}"
mkdir -p "$OUT"

# BENCH_PAM_RUNTIME=/path/to/runtime benchmarks an unreleased PAM build.
if [ -n "${BENCH_PAM_RUNTIME:-}" ]; then
    PAM_VERSION="local-$("$BENCH_PAM_RUNTIME/bin/pam-run" --version | awk '{print $2}')"
fi

images() {
    if [ -n "${BENCH_PAM_RUNTIME:-}" ]; then
        docker build -q -t "pam-octane-bench/pam:$PAM_VERSION" \
            -f "$BENCH_DIR/docker/pam-local.Dockerfile" "$BENCH_PAM_RUNTIME" >/dev/null
    else
        docker build -q -t "pam-octane-bench/pam:$PAM_VERSION" --build-arg PAM_VERSION="$PAM_VERSION" \
            -f "$BENCH_DIR/docker/pam.Dockerfile" "$BENCH_DIR/docker" >/dev/null
    fi
    docker build -q -t "pam-octane-bench/frankenphp:$FRANKENPHP_VERSION" --build-arg FRANKENPHP_VERSION="$FRANKENPHP_VERSION" \
        -f "$BENCH_DIR/docker/frankenphp.Dockerfile" "$BENCH_DIR/docker" >/dev/null
    docker build -q -t "pam-octane-bench/php-octane:$SWOOLE_VERSION-$ROADRUNNER_VERSION" --build-arg PHP_IMAGE="$PHP_IMAGE" \
        --build-arg SWOOLE_VERSION="$SWOOLE_VERSION" --build-arg ROADRUNNER_VERSION="$ROADRUNNER_VERSION" \
        -f "$BENCH_DIR/docker/php-octane.Dockerfile" "$BENCH_DIR/docker" >/dev/null
}

app() {
    provision_app "$APP" "$LARAVEL_CONSTRAINT"
    (
        cd "$APP"
        if [ ! -d vendor/spiral/roadrunner-http ]; then
            "$PAM" composer require --no-interaction -W spiral/roadrunner-http spiral/roadrunner-cli >/dev/null 2>&1
        fi
        touch .rr.yaml
        sed -i 's/^APP_ENV=.*/APP_ENV=production/; s/^APP_DEBUG=.*/APP_DEBUG=false/; s/^LOG_LEVEL=.*/LOG_LEVEL=error/' .env
        "$PAM" artisan optimize >/dev/null
    )
}

spec() {
    python3 - "$OUT/machine.json" <<'PY'
import json, platform, subprocess, sys
def sh(cmd):
    try:
        return subprocess.run(cmd, shell=True, capture_output=True, text=True, timeout=20, env={"LC_ALL": "C", "PATH": "/usr/bin:/bin:/usr/local/bin"}).stdout.strip()
    except Exception as error:
        return str(error)
json.dump({
    "kernel": platform.release(),
    "cpu": sh("lscpu | sed -n 's/^Model name:\\s*//p'"),
    "threads": sh("nproc"),
    "memory": sh("free -h | awk '/Mem:/{print $2}'"),
    "governor": sh("cat /sys/devices/system/cpu/cpu0/cpufreq/scaling_governor"),
    "docker": sh("docker version --format '{{.Server.Version}}'"),
    "wrk": sh("wrk --version 2>&1 | head -1"),
    "images": sh("docker images --format '{{.Repository}}:{{.Tag}} {{.ID}}' | grep pam-octane-bench"),
    "tcp_migrate_req": sh("cat /proc/sys/net/ipv4/tcp_migrate_req"),
}, open(sys.argv[1], "w"), indent=2)
PY
}

start_server() {
    local server="$1" image cmd
    local -a run=(docker run -d --rm --name "bench-$server" --network host --cpuset-cpus "$SERVER_CPUS"
        --user "$(id -u):$(id -g)" -e HOME=/tmp -v "$APP:$APP" -w "$APP")
    # Same path inside and outside: `artisan optimize` caches absolute paths.
    case "$server" in
        pam) "${run[@]}" "pam-octane-bench/pam:$PAM_VERSION" \
                pam start artisan --workers "$WORKERS" -- pam:octane --host=127.0.0.1 --port="$PORT" ;;
        frankenphp) "${run[@]}" "pam-octane-bench/frankenphp:$FRANKENPHP_VERSION" \
                php artisan octane:start --server=frankenphp --host=127.0.0.1 --port="$PORT" \
                --workers="$WORKERS" --max-requests=10000000 --admin-port=2119 ;;
        swoole) "${run[@]}" "pam-octane-bench/php-octane:$SWOOLE_VERSION-$ROADRUNNER_VERSION" \
                php artisan octane:start --server=swoole --host=127.0.0.1 --port="$PORT" \
                --workers="$WORKERS" --task-workers=1 --max-requests=10000000 ;;
        roadrunner) "${run[@]}" "pam-octane-bench/php-octane:$SWOOLE_VERSION-$ROADRUNNER_VERSION" \
                php artisan octane:start --server=roadrunner --host=127.0.0.1 --port="$PORT" \
                --workers="$WORKERS" --max-requests=10000000 --rpc-port=6101 ;;
    esac >/dev/null
    wait_http "http://127.0.0.1:$PORT/_r/hello" 90
}

memory_bytes() {
    local id cg
    id=$(docker inspect -f '{{.Id}}' "bench-$1")
    for cg in "/sys/fs/cgroup/system.slice/docker-$id.scope" "/sys/fs/cgroup/docker/$id"; do
        if [ -r "$cg/memory.current" ]; then
            echo "$(cat "$cg/memory.current") $(cat "$cg/memory.peak" 2>/dev/null || echo 0)"
            return
        fi
    done
    echo "0 0"
}

images
app
spec
echo "server,endpoint,rps,p50_ms,p99_ms,errors,memory_current_mb,memory_peak_mb" > "$OUT/results.csv"
for server in $SERVERS; do
    log "benchmark: $server"
    docker rm -f "bench-$server" >/dev/null 2>&1 || true
    start_server "$server"
    for endpoint in $ENDPOINTS; do
        url="http://127.0.0.1:$PORT/_r/$endpoint"
        taskset -c "$LOAD_CPUS" wrk -t"$WRK_THREADS" -c"$WRK_CONNECTIONS" -d"$WARMUP" "$url" >/dev/null 2>&1
        taskset -c "$LOAD_CPUS" wrk -t"$WRK_THREADS" -c"$WRK_CONNECTIONS" -d"$DURATION" --latency "$url" \
            > "$OUT/$server-$endpoint.wrk.txt" 2>&1
        read -r current peak < <(memory_bytes "$server")
        python3 - "$OUT/$server-$endpoint.wrk.txt" "$server" "$endpoint" "$current" "$peak" >> "$OUT/results.csv" <<'PY'
import re, sys
text = open(sys.argv[1]).read()
def latency(label):
    m = re.search(rf"^\s*{label}%\s+([\d.]+)(us|ms|s)", text, re.M)
    if not m:
        return ""
    value, unit = float(m.group(1)), m.group(2)
    return round(value / 1000 if unit == "us" else value * 1000 if unit == "s" else value, 2)
rps = re.search(r"Requests/sec:\s+([\d.]+)", text)
errors = sum(int(x) for x in re.findall(r"(?:connect|read|write|timeout) (\d+)", text))
non2xx = re.search(r"Non-2xx or 3xx responses: (\d+)", text)
errors += int(non2xx.group(1)) if non2xx else 0
print(",".join(map(str, [sys.argv[2], sys.argv[3], rps.group(1) if rps else "", latency("50"), latency("99"),
                         errors, round(int(sys.argv[4]) / 2**20), round(int(sys.argv[5]) / 2**20)])))
PY
        tail -1 "$OUT/results.csv"
    done
    docker logs "bench-$server" > "$OUT/$server.log" 2>&1 || true
    docker rm -f "bench-$server" >/dev/null
    sleep 2
done
python3 "$BENCH_DIR/table.py" "$OUT/results.csv" | tee "$OUT/results.md"
