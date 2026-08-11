#!/usr/bin/env bash

set -euo pipefail

TARGET="${1:?root process PID or container:name is required}"
OUTPUT="${2:?output CSV path is required}"
INTERVAL="${PAM_BENCH_SAMPLE_INTERVAL:-1}"

printf 'timestamp_ms,rss_bytes,cpu_percent,processes\n' >"$OUTPUT"

process_tree() {
    local root_pid="$1"
    local pending=("$root_pid")
    local found=()
    local pid children child
    while ((${#pending[@]})); do
        pid="${pending[0]}"
        pending=("${pending[@]:1}")
        [[ -r "/proc/$pid/status" ]] || continue
        found+=("$pid")
        children="$(pgrep -P "$pid" 2>/dev/null || true)"
        for child in $children; do
            pending+=("$child")
        done
    done
    (IFS=,; printf '%s' "${found[*]}")
}

while true; do
    if [[ "$TARGET" == container:* ]]; then
        container="${TARGET#container:}"
        [[ "$(docker inspect --format '{{.State.Running}}' "$container" 2>/dev/null || true)" == "true" ]] || break
        pids="$(docker top "$container" -eo pid 2>/dev/null | awk 'NR > 1 {values[++count] = $1} END {for (i = 1; i <= count; i++) printf "%s%s", values[i], i == count ? "" : ","}')"
    else
        kill -0 "$TARGET" 2>/dev/null || break
        pids="$(process_tree "$TARGET")"
    fi
    [[ -n "$pids" ]] || break
    read -r rss_kib cpu processes < <(
        ps -o rss=,pcpu= -p "$pids" 2>/dev/null |
            awk '{rss += $1; cpu += $2; count += 1} END {printf "%.0f %.3f %d\n", rss, cpu, count}'
    )
    timestamp_ms="$(date +%s%3N)"
    printf '%s,%s,%s,%s\n' "$timestamp_ms" "$((rss_kib * 1024))" "$cpu" "$processes" >>"$OUTPUT"
    sleep "$INTERVAL"
done
