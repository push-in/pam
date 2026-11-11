#!/usr/bin/env bash
# Local A/B regression benchmark of PAM builds on one readiness app (no Docker):
#   ab-local.sh APP_DIR "label=/path/to/pam-run[:extra flags]" ...
# Each variant gets its own process-manager state; variants alternate for
# ROUNDS rounds; the server is pinned to SERVER_CPUS and wrk to LOAD_CPUS.
set -euo pipefail
APP="$1"; shift
ROUNDS="${ROUNDS:-2}"; WORKERS="${WORKERS:-8}"; PORT="${PORT:-8450}"
SERVER_CPUS="${SERVER_CPUS:-0-7}"; LOAD_CPUS="${LOAD_CPUS:-8-11}"
DURATION="${DURATION:-20s}"; WARMUP="${WARMUP:-5s}"
ENDPOINTS="${ENDPOINTS:-hello json db view}"
STATE_ROOT="${STATE_ROOT:-$(mktemp -d)}"
cd "$APP"
for round in $(seq 1 "$ROUNDS"); do
    for variant in "$@"; do
        label="${variant%%=*}"; spec="${variant#*=}"; pam="${spec%%:*}"; flags=""
        [[ "$spec" == *:* ]] && flags="${spec#*:}"
        state="$STATE_ROOT/$label"; mkdir -p "$state/runtime"; chmod 700 "$state/runtime"
        export PAM_MANAGER_STATE_DIR="$state" PAM_MANAGER_RUNTIME_DIR="$state/runtime"
        # shellcheck disable=SC2086
        taskset -c "$SERVER_CPUS" "$pam" octane:start --port="$PORT" --workers="$WORKERS" --max-requests=10000000 $flags >/dev/null
        until curl -fs -o /dev/null "http://127.0.0.1:$PORT/_r/hello"; do sleep 0.3; done
        for endpoint in $ENDPOINTS; do
            taskset -c "$LOAD_CPUS" wrk -t4 -c64 -d"$WARMUP" "http://127.0.0.1:$PORT/_r/$endpoint" >/dev/null
            out=$(taskset -c "$LOAD_CPUS" wrk -t4 -c64 -d"$DURATION" --latency "http://127.0.0.1:$PORT/_r/$endpoint")
            rps=$(awk '/Requests\/sec/ {print $2}' <<<"$out")
            p99=$(awk '$1=="99%" {print $2}' <<<"$out")
            errors=$(grep -c "Socket errors\|Non-2xx" <<<"$out" || true)
            printf '%s\t%s\t%s\t%s\t%s\terrors=%s\n' "$round" "$label" "$endpoint" "$rps" "$p99" "$errors"
        done
        "$pam" octane:stop >/dev/null
        "$pam" daemon stop >/dev/null 2>&1 || true
    done
done
