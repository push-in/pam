#!/usr/bin/env bash
set -euo pipefail

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
results=${PAM_RECOVERY_EXTENSION_RESULTS:-"${root}/benchmarks/process-manager/results/extension-profile"}
rounds=${PAM_RECOVERY_EXTENSION_ROUNDS:-${PAM_RECOVERY_ROUNDS:-10}}
pam_binary=${PAM_BENCH_BINARY:-"${root}/target/release/pam"}

[[ ! -L ${results} ]] || { printf 'refusing symlink extension-profile results directory\n' >&2; exit 1; }
[[ ! -e ${results} ]] || { printf 'refusing to overwrite extension-profile evidence: %s\n' "${results}" >&2; exit 1; }
mkdir -p "${results}"

profile_status=0
for profile in compatible isolated; do
    extensions=
    if [[ ${profile} == isolated ]]; then
        extensions=iconv
    fi
    printf 'Measuring 16-worker PAM recovery with %s extension profile\n' "${profile}"
    if ! PAM_RECOVERY_RESULTS="${results}/${profile}" \
    PAM_RECOVERY_ROUNDS="${rounds}" \
    PAM_RECOVERY_WORKERS=16 \
    PAM_RECOVERY_PHP_EXTENSIONS="${extensions}" \
    PAM_RECOVERY_MAX_P95_MILLIS=650 \
    PAM_RECOVERY_MAX_BACKOFF_P95_MILLIS=250 \
    PAM_RECOVERY_MAX_READINESS_P95_MILLIS=550 \
    PAM_BENCH_BINARY="${pam_binary}" \
        "${root}/benchmarks/process-manager/run.sh"; then
        profile_status=1
    fi
done

set +e
"${pam_binary}" "${root}/benchmarks/process-manager/extension-profile-report.php" "${results}"
report_status=$?
set -e
"${pam_binary}" "${root}/benchmarks/octane/evidence-manifest.php" "${results}" 8 >/dev/null
"${pam_binary}" "${root}/benchmarks/octane/evidence-manifest.php" "${results}" 8 --verify
(( profile_status == 0 && report_status == 0 ))
