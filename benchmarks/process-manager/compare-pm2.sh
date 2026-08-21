#!/usr/bin/env bash
set -euo pipefail

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
results=${PAM_PM2_RESULTS:-"${root}/benchmarks/process-manager/results/comparison"}
rounds=${PAM_RECOVERY_ROUNDS:-10}
pm2_root="${root}/benchmarks/process-manager/pm2"
pm2="${pm2_root}/node_modules/.bin/pm2"

[[ ${rounds} =~ ^[0-9]+$ ]] && (( rounds >= 3 && rounds <= 100 )) || {
    printf 'PAM_RECOVERY_ROUNDS must be an integer from 3 to 100\n' >&2
    exit 64
}
[[ -x ${pm2} ]] || {
    printf 'locked PM2 dependencies are missing; run npm ci --ignore-scripts in %s\n' "${pm2_root}" >&2
    exit 69
}
[[ ! -L ${results} ]] || { printf 'refusing symlink comparison directory\n' >&2; exit 1; }
mkdir -p "${results}"
for artifact in comparison-report.json metadata.json evidence-manifest.json; do
    [[ ! -e ${results}/${artifact} ]] || {
        printf 'refusing to overwrite comparison artifact: %s\n' "${results}/${artifact}" >&2
        exit 1
    }
done

PAM_RECOVERY_RESULTS="${results}/pam" \
PAM_RECOVERY_ROUNDS="${rounds}" \
PAM_RECOVERY_MAX_P95_MILLIS="${PAM_RECOVERY_MAX_P95_MILLIS:-300}" \
PAM_RECOVERY_MAX_RSS_GROWTH_BYTES="${PAM_RECOVERY_MAX_RSS_GROWTH_BYTES:-16777216}" \
    "${root}/benchmarks/process-manager/run.sh"

scratch=$(mktemp -d)
export PM2_HOME="${scratch}/pm2-home"
name="pm2-recovery-benchmark-$$"
cleanup() {
    "${pm2}" delete "${name}" >/dev/null 2>&1 || true
    "${pm2}" kill >/dev/null 2>&1 || true
    rm -rf -- "${scratch}"
}
trap cleanup EXIT INT TERM

"${pm2}" start "${pm2_root}/fixture.php" --name "${name}" --interpreter php \
    --restart-delay 10 --no-vizion >/dev/null
daemon_pid=$(<"${PM2_HOME}/pm2.pid")
[[ ${daemon_pid} =~ ^[0-9]+$ && -r /proc/${daemon_pid}/status ]] || {
    printf 'cannot resolve PM2 daemon PID\n' >&2
    exit 1
}
rss_bytes() {
    awk '/^VmRSS:/ {print $2 * 1024}' "/proc/${daemon_pid}/status"
}
rss_before=$(rss_bytes)
mkdir -p "${results}/pm2"
printf 'round,recovery_millis,success\n' >"${results}/pm2/recovery.csv"
for (( round = 1; round <= rounds; round++ )); do
    "${pm2}" restart "${name}" >/dev/null
    original_pid=$("${pm2}" pid "${name}" | tail -n 1)
    [[ ${original_pid} =~ ^[0-9]+$ && ${original_pid} -gt 0 ]] || {
        printf 'cannot resolve PM2 application PID\n' >&2
        exit 1
    }
    started=$(date +%s%3N)
    kill -KILL "${original_pid}"
    deadline=$(( started + 10000 ))
    success=0
    recovered_at=${deadline}
    while (( $(date +%s%3N) < deadline )); do
        current_pid=$("${pm2}" pid "${name}" 2>/dev/null | tail -n 1 || true)
        if [[ ${current_pid} =~ ^[0-9]+$ && ${current_pid} -gt 0 && ${current_pid} != "${original_pid}" ]]; then
            success=1
            recovered_at=$(date +%s%3N)
            break
        fi
        sleep 0.01
    done
    printf '%d,%d,%d\n' "${round}" "$(( recovered_at - started ))" "${success}" \
        >>"${results}/pm2/recovery.csv"
done
rss_after=$(rss_bytes)
printf '{"daemon_rss_before_bytes":%d,"daemon_rss_after_bytes":%d}\n' \
    "${rss_before}" "${rss_after}" >"${results}/pm2/resources.json"

commit=$(git -C "${root}" rev-parse HEAD)
dirty=false
git -C "${root}" diff --quiet --ignore-submodules HEAD -- || dirty=true
native_commit=$(git -C "${root}/pam-native" rev-parse HEAD)
[[ ${native_commit} =~ ^[0-9a-f]{40}$ ]] || {
    printf 'cannot resolve pinned PAM Native commit\n' >&2
    exit 1
}
kernel=$(uname -srmo | php -r 'echo json_encode(trim(stream_get_contents(STDIN)), JSON_THROW_ON_ERROR);')
pm2_version=$("${pm2}" --version | tail -n 1)
pm2_integrity=$(php -r '$v=json_decode(file_get_contents($argv[1]),true,flags:JSON_THROW_ON_ERROR); echo $v["packages"]["node_modules/pm2"]["integrity"];' "${pm2_root}/package-lock.json")
printf '{"schema_version":1,"source":{"commit":"%s","dirty":%s,"dirty_scope":"tracked_files"},"host":{"kernel":%s},"tools":{"pam_sha256":"%s","pam_native_commit":"%s","pm2_version":"%s","pm2_integrity":"%s","php_version":"%s"},"parameters":{"rounds":%d,"restart_delay_millis":10,"poll_interval_millis":10,"crash_signal":9,"instances":1}}\n' \
    "${commit}" "${dirty}" "${kernel}" \
    "$(sha256sum "${PAM_BENCH_BINARY:-${root}/target/release/pam}" | awk '{print $1}')" \
    "${native_commit}" "${pm2_version}" "${pm2_integrity}" "$(php -r 'echo PHP_VERSION;')" "${rounds}" \
    >"${results}/metadata.json"

php "${root}/benchmarks/process-manager/comparison-report.php" "${results}"
php "${root}/benchmarks/octane/evidence-manifest.php" "${results}" 6 >/dev/null
php "${root}/benchmarks/octane/evidence-manifest.php" "${results}" 6 --verify
