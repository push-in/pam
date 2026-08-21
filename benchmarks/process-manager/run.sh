#!/usr/bin/env bash
set -euo pipefail

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
results=${PAM_RECOVERY_RESULTS:-"${root}/benchmarks/process-manager/results"}
pam_binary=${PAM_BENCH_BINARY:-"${root}/target/release/pam"}
rounds=${PAM_RECOVERY_ROUNDS:-10}
workers=${PAM_RECOVERY_WORKERS:-1}
maximum_p95_millis=${PAM_RECOVERY_MAX_P95_MILLIS:-200}
maximum_detection_p95_millis=${PAM_RECOVERY_MAX_DETECTION_P95_MILLIS:-10}
maximum_backoff_p95_millis=${PAM_RECOVERY_MAX_BACKOFF_P95_MILLIS:-20}
maximum_readiness_p95_millis=${PAM_RECOVERY_MAX_READINESS_P95_MILLIS:-150}
maximum_rss_growth_bytes=${PAM_RECOVERY_MAX_RSS_GROWTH_BYTES:-16777216}

[[ ${rounds} =~ ^[0-9]+$ ]] && (( rounds >= 3 && rounds <= 100 )) || {
    printf 'PAM_RECOVERY_ROUNDS must be an integer from 3 to 100\n' >&2
    exit 64
}
[[ ${workers} =~ ^[0-9]+$ ]] && (( workers >= 1 && workers <= 64 )) || {
    printf 'PAM_RECOVERY_WORKERS must be an integer from 1 to 64\n' >&2
    exit 64
}
for value in "${maximum_p95_millis}" "${maximum_detection_p95_millis}" \
    "${maximum_backoff_p95_millis}" "${maximum_readiness_p95_millis}" \
    "${maximum_rss_growth_bytes}"; do
    [[ ${value} =~ ^[0-9]+$ ]] || {
        printf 'recovery thresholds must be non-negative integers\n' >&2
        exit 64
    }
done
[[ ! -L ${results} ]] || { printf 'refusing symlink results directory\n' >&2; exit 1; }
mkdir -p "${results}"
for artifact in recovery.csv recovery-phases.csv resources.json metadata.json recovery-report.json evidence-manifest.json launch-error.log application-error.log; do
    [[ ! -e ${results}/${artifact} ]] || {
        printf 'refusing to overwrite recovery artifact: %s\n' "${results}/${artifact}" >&2
        exit 1
    }
done
if [[ ! -x ${pam_binary} ]]; then
    cargo build --locked --release --manifest-path "${root}/Cargo.toml"
fi

scratch=$(mktemp -d)
export PAM_MANAGER_STATE_DIR="${scratch}/state"
export PAM_MANAGER_RUNTIME_DIR="${scratch}/runtime"
name="recovery-benchmark-$$"
cleanup() {
    "${pam_binary}" stop "${name}" >/dev/null 2>&1 || true
    "${pam_binary}" delete "${name}" >/dev/null 2>&1 || true
    "${pam_binary}" daemon stop >/dev/null 2>&1 || true
    rm -rf -- "${scratch}"
}
trap cleanup EXIT INT TERM

port=$(php -r '$socket = stream_socket_server("tcp://127.0.0.1:0", $error, $message); if ($socket === false) { exit(1); } echo (int) parse_url(stream_socket_get_name($socket, false), PHP_URL_PORT);')
export PAM_TEST_PORT="${port}"
"${pam_binary}" daemon start >/dev/null
daemon_pid=$("${pam_binary}" daemon status | php -r '$text = stream_get_contents(STDIN); if (!preg_match("/PID ([0-9]+)/", $text, $match)) { exit(1); } echo $match[1];')
rss_bytes() {
    awk '/^VmRSS:/ {print $2 * 1024}' "/proc/${daemon_pid}/status"
}
rss_before=$(rss_bytes)
set +e
launch_output=$(
    cd "${root}"
    "${pam_binary}" up tests/fixtures/server.php --name "${name}" --workers "${workers}" \
        --restart-delay-ms 10 --restart-backoff-max-ms 100 \
        --max-unstable-restarts 100 --min-uptime-ms 1000 2>&1
)
launch_status=$?
set -e
if (( launch_status != 0 )); then
    printf '%s\n' "${launch_output}" | tail -c 65536 >"${results}/launch-error.log"
    application_error="${PAM_MANAGER_STATE_DIR}/logs/${name}.error.log"
    if [[ -f ${application_error} && ! -L ${application_error} ]]; then
        tail -c 1048576 "${application_error}" >"${results}/application-error.log"
    fi
    printf 'managed application failed to launch; diagnostics retained in %s\n' \
        "${results}" >&2
    exit "${launch_status}"
fi

printf 'round,recovery_millis,success\n' >"${results}/recovery.csv"
printf 'round,detection_millis,backoff_millis,readiness_millis,accounted_millis,success\n' \
    >"${results}/recovery-phases.csv"
for (( round = 1; round <= rounds; round++ )); do
    "${pam_binary}" restart "${name}" >/dev/null
    original_pid=$("${pam_binary}" status "${name}" --json | php -r '$value = json_decode(stream_get_contents(STDIN), true, flags: JSON_THROW_ON_ERROR); echo $value["pid"];')
    started=$(date +%s%3N)
    kill -KILL "${original_pid}"
    deadline=$(( started + 10000 ))
    success=0
    recovered_at=${deadline}
    while (( $(date +%s%3N) < deadline )); do
        snapshot=$("${pam_binary}" status "${name}" --json 2>/dev/null || true)
        current_pid=$(php -r '$value = json_decode(stream_get_contents(STDIN), true); echo is_array($value) ? ($value["pid"] ?? "") : "";' <<<"${snapshot}")
        if [[ -n ${current_pid} && ${current_pid} != "${original_pid}" ]]; then
            success=1
            recovered_at=$(date +%s%3N)
            break
        fi
        sleep 0.01
    done
    printf '%d,%d,%d\n' "${round}" "$(( recovered_at - started ))" "${success}" >>"${results}/recovery.csv"
    if (( success == 1 )); then
        phase_row=$(php -r '
            $value = json_decode(stream_get_contents(STDIN), true, flags: JSON_THROW_ON_ERROR);
            $recovery = $value["recovery"] ?? null;
            $detected = $recovery["lastExitDetectedAtMillis"] ?? null;
            $restarted = $recovery["lastRecoveryStartedAtMillis"] ?? null;
            $ready = $recovery["lastRecoveryReadyAtMillis"] ?? null;
            $started = filter_var($argv[2], FILTER_VALIDATE_INT);
            if (!is_array($recovery) || !is_int($detected) || !is_int($restarted)
                || !is_int($ready) || $started === false || $detected < $started
                || $restarted < $detected || $ready < $restarted) { exit(1); }
            printf("%d,%d,%d,%d,%d,1", (int) $argv[1], $detected - $started,
                $restarted - $detected, $ready - $restarted, $ready - $started);
        ' "${round}" "${started}" <<<"${snapshot}") || {
            printf 'recovery phase contract is missing or invalid in round %d\n' "${round}" >&2
            exit 1
        }
        printf '%s\n' "${phase_row}" >>"${results}/recovery-phases.csv"
    else
        printf '%d,0,0,0,0,0\n' "${round}" >>"${results}/recovery-phases.csv"
    fi
done
rss_after=$(rss_bytes)
printf '{"daemon_rss_before_bytes":%d,"daemon_rss_after_bytes":%d}\n' \
    "${rss_before}" "${rss_after}" >"${results}/resources.json"

commit=$(git -C "${root}" rev-parse HEAD)
dirty=false
git -C "${root}" diff --quiet --ignore-submodules HEAD -- || dirty=true
native_commit=$(git -C "${root}/pam-native" rev-parse HEAD)
[[ ${native_commit} =~ ^[0-9a-f]{40}$ ]] || {
    printf 'cannot resolve pinned PAM Native commit\n' >&2
    exit 1
}
kernel=$(uname -srmo | php -r 'echo json_encode(trim(stream_get_contents(STDIN)), JSON_THROW_ON_ERROR);')
binary_sha=$(sha256sum "${pam_binary}" | awk '{print $1}')
printf '{"schema_version":1,"source":{"commit":"%s","dirty":%s,"dirty_scope":"tracked_files"},"host":{"kernel":%s},"tools":{"pam_sha256":"%s","pam_native_commit":"%s"},"parameters":{"rounds":%d,"workers":%d,"restart_delay_millis":10,"poll_interval_millis":10,"maximum_p95_millis":%d,"maximum_detection_p95_millis":%d,"maximum_backoff_p95_millis":%d,"maximum_readiness_p95_millis":%d,"maximum_rss_growth_bytes":%d}}\n' \
    "${commit}" "${dirty}" "${kernel}" "${binary_sha}" "${native_commit}" "${rounds}" \
    "${workers}" \
    "${maximum_p95_millis}" "${maximum_detection_p95_millis}" \
    "${maximum_backoff_p95_millis}" "${maximum_readiness_p95_millis}" \
    "${maximum_rss_growth_bytes}" >"${results}/metadata.json"

set +e
"${pam_binary}" "${root}/benchmarks/process-manager/recovery-report.php" \
    "${results}" "${maximum_p95_millis}" "${maximum_rss_growth_bytes}" \
    "${maximum_detection_p95_millis}" "${maximum_backoff_p95_millis}" \
    "${maximum_readiness_p95_millis}"
report_status=$?
set -e
"${pam_binary}" "${root}/benchmarks/octane/evidence-manifest.php" "${results}" 5 >/dev/null
"${pam_binary}" "${root}/benchmarks/octane/evidence-manifest.php" "${results}" 5 --verify
exit "${report_status}"
