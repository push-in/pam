#!/usr/bin/env bash
set -euo pipefail

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
results=${PAM_RECOVERY_MATRIX_RESULTS:-"${root}/benchmarks/process-manager/results/worker-matrix"}
rounds=${PAM_RECOVERY_MATRIX_ROUNDS:-${PAM_RECOVERY_ROUNDS:-10}}

[[ ! -L ${results} ]] || { printf 'refusing symlink matrix results directory\n' >&2; exit 1; }
[[ ! -e ${results} ]] || { printf 'refusing to overwrite worker-matrix evidence: %s\n' "${results}" >&2; exit 1; }
mkdir -p "${results}"

matrix_status=0
for workers in 1 4 16; do
    case ${workers} in
        1) maximum_p95=200; maximum_readiness_p95=150 ;;
        4) maximum_p95=250; maximum_readiness_p95=200 ;;
        16) maximum_p95=650; maximum_readiness_p95=550 ;;
    esac
    printf 'Measuring PAM recovery with %d worker(s)\n' "${workers}"
    if ! PAM_RECOVERY_RESULTS="${results}/workers-${workers}" \
    PAM_RECOVERY_ROUNDS="${rounds}" \
    PAM_RECOVERY_WORKERS="${workers}" \
    PAM_RECOVERY_MAX_P95_MILLIS="${maximum_p95}" \
    PAM_RECOVERY_MAX_READINESS_P95_MILLIS="${maximum_readiness_p95}" \
        "${root}/benchmarks/process-manager/run.sh"; then
        matrix_status=1
    fi
done

set +e
"${PAM_BENCH_BINARY:-${root}/target/release/pam}" \
    "${root}/benchmarks/process-manager/worker-matrix-report.php" "${results}"
report_status=$?
set -e
"${PAM_BENCH_BINARY:-${root}/target/release/pam}" \
    "${root}/benchmarks/octane/evidence-manifest.php" "${results}" 7 >/dev/null
"${PAM_BENCH_BINARY:-${root}/target/release/pam}" \
    "${root}/benchmarks/octane/evidence-manifest.php" "${results}" 7 --verify
(( matrix_status == 0 && report_status == 0 ))
