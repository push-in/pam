#!/usr/bin/env bash
set -euo pipefail

repository=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
compose_file="${repository}/benchmarks/laravel/compose.yml"
rounds="${PAM_BENCH_ROUNDS:-3}"
warmup="${PAM_BENCH_WARMUP_SECONDS:-10}"
duration="${PAM_BENCH_DURATION_SECONDS:-30}"
threads="${PAM_BENCH_THREADS:-4}"
connections="${PAM_BENCH_CONNECTIONS:-64}"
workers="${PAM_BENCH_WORKERS:-4}"
endpoint="${PAM_BENCH_ENDPOINT:-/api/ping}"
cpu_count=$(nproc)
if (( cpu_count >= 4 )); then
    default_app_cpu_set="0,1"
    default_load_cpu_set="2,3"
elif (( cpu_count >= 2 )); then
    default_app_cpu_set="0,1"
    default_load_cpu_set="0,1"
else
    default_app_cpu_set="0"
    default_load_cpu_set="0"
fi
app_cpu_set="${PAM_BENCH_APP_CPUSET:-${default_app_cpu_set}}"
load_cpu_set="${PAM_BENCH_LOAD_CPUSET:-${default_load_cpu_set}}"
export PAM_BENCH_APP_CPUSET="${app_cpu_set}"
pam_memory_limit_kb="${PAM_BENCH_MEMORY_LIMIT_KB:-1048576}"
stamp=$(date -u +%Y%m%dT%H%M%SZ)
results="${PAM_BENCH_RESULTS:-${repository}/benchmarks/results/${stamp}}"
pam_pid=""

mkdir -p "${results}"

cleanup() {
    if [[ -n "${pam_pid}" ]]; then
        kill -TERM "${pam_pid}" 2>/dev/null || true
        wait "${pam_pid}" 2>/dev/null || true
    fi
    docker compose -f "${compose_file}" down --remove-orphans >/dev/null 2>&1 || true
}
trap cleanup EXIT

for command in docker wrk curl php taskset; do
    command -v "${command}" >/dev/null || {
        printf 'Missing benchmark dependency: %s\n' "${command}" >&2
        exit 1
    }
done

if [[ ! -x "${repository}/target/release/pam" ]]; then
    cargo build --locked --release --manifest-path "${repository}/Cargo.toml"
fi

php -r '
$data = [
    "git_commit" => trim((string) shell_exec("git rev-parse HEAD")),
    "generated_at" => gmdate(DATE_ATOM),
    "kernel" => php_uname(),
    "cpu_count" => (int) trim((string) shell_exec("nproc")),
    "php" => PHP_VERSION,
];
file_put_contents($argv[1], json_encode($data, JSON_PRETTY_PRINT | JSON_THROW_ON_ERROR).PHP_EOL);
' "${results}/metadata.json"

wait_ready() {
    local url=$1
    for _ in $(seq 1 120); do
        if curl --fail --silent --show-error "${url}" >/dev/null 2>&1; then
            return 0
        fi
        sleep 1
    done
    printf 'Runtime did not become ready: %s\n' "${url}" >&2
    return 1
}

run_load() {
    local name=$1
    local url=$2
    taskset --cpu-list "${load_cpu_set}" wrk -t"${threads}" -c"${connections}" -d"${warmup}s" "${url}" >/dev/null
    for round in $(seq 1 "${rounds}"); do
        taskset --cpu-list "${load_cpu_set}" wrk -t"${threads}" -c"${connections}" -d"${duration}s" \
            -s "${repository}/benchmarks/laravel/wrk.lua" "${url}" |
            tail -n 1 >"${results}/${name}.round-${round}.json"
    done
}

run_container() {
    local service=$1
    local name=$2
    local port=$3
    local container_ids=()
    docker compose -f "${compose_file}" up --build --detach "${service}"
    wait_ready "http://127.0.0.1:${port}${endpoint}"
    run_load "${name}" "http://127.0.0.1:${port}${endpoint}"
    mapfile -t container_ids < <(docker compose -f "${compose_file}" ps --quiet)
    docker stats --no-stream --format '{{json .}}' "${container_ids[@]}" |
        php -r '
$rows = [];
while (($line = fgets(STDIN)) !== false) {
    $row = json_decode($line, true);
    if (is_array($row)) {
        $rows[] = $row;
    }
}
file_put_contents($argv[1], json_encode($rows, JSON_PRETTY_PRINT | JSON_THROW_ON_ERROR).PHP_EOL);
' "${results}/${name}.memory.json"
    docker compose -f "${compose_file}" down --remove-orphans
}

run_container nginx php-fpm 18080
run_container octane octane-swoole 18081
run_container frankenphp frankenphp 18082
run_container roadrunner roadrunner 18083

(
    cd "${repository}"
    APP_ENV=production \
    APP_DEBUG=false \
    APP_KEY=base64:MDEyMzQ1Njc4OWFiY2RlZjAxMjM0NTY3ODlhYmNkZWY= \
    DB_CONNECTION=sqlite \
    DB_DATABASE=:memory: \
    CACHE_STORE=array \
    SESSION_DRIVER=array \
    PAM_LARAVEL_OBSERVABILITY=false \
    PAM_STATE_GUARD=false \
    PAM_LARAVEL_SMOKE_PORT=18084 \
    PAM_BENCH_WORKERS="${workers}" \
        taskset --cpu-list "${app_cpu_set}" target/release/pam start compat/laravel-smoke/pam.php \
        --workers "${workers}" \
        --admin-address 127.0.0.1:19084
) >"${results}/pam.log" 2>&1 &
pam_pid=$!
wait_ready "http://127.0.0.1:18084${endpoint}"
run_load pam "http://127.0.0.1:18084${endpoint}"
pam_processes=("${pam_pid}")
process_index=0
while (( process_index < ${#pam_processes[@]} )); do
    mapfile -t children < <(pgrep -P "${pam_processes[$process_index]}" || true)
    pam_processes+=("${children[@]}")
    process_index=$((process_index + 1))
done
total_rss_kb=0
for process in "${pam_processes[@]}"; do
    rss_kb=$(awk '/^VmRSS:/ { print $2 }' "/proc/${process}/status" 2>/dev/null || true)
    total_rss_kb=$((total_rss_kb + ${rss_kb:-0}))
done
php -r '
$report = [
    "processes" => array_map("intval", array_slice($argv, 4)),
    "rss_kilobytes" => (int) $argv[1],
    "limit_kilobytes" => (int) $argv[2],
];
file_put_contents($argv[3], json_encode($report, JSON_PRETTY_PRINT | JSON_THROW_ON_ERROR).PHP_EOL);
' "${total_rss_kb}" "${pam_memory_limit_kb}" "${results}/pam.memory.json" "${pam_processes[@]}"
if (( total_rss_kb > pam_memory_limit_kb )); then
    printf 'PAM exceeded benchmark memory contract: %s KiB > %s KiB\n' "${total_rss_kb}" "${pam_memory_limit_kb}" >&2
    exit 1
fi
kill -TERM "${pam_pid}"
wait "${pam_pid}" || true
pam_pid=""

PAM_BENCH_WARMUP_SECONDS="${warmup}" \
PAM_BENCH_DURATION_SECONDS="${duration}" \
PAM_BENCH_THREADS="${threads}" \
PAM_BENCH_CONNECTIONS="${connections}" \
PAM_BENCH_WORKERS="${workers}" \
PAM_BENCH_APP_CPUSET="${app_cpu_set}" \
PAM_BENCH_LOAD_CPUSET="${load_cpu_set}" \
PAM_BENCH_MEMORY_LIMIT_KB="${pam_memory_limit_kb}" \
    php "${repository}/benchmarks/laravel/aggregate.php" "${results}"
