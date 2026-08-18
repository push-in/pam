#!/usr/bin/env bash

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
MATRIX_RESULTS="${PAM_BENCH_MATRIX_RESULTS:-$ROOT/benchmarks/octane/results/matrix}"
WORKER_MATRIX="${PAM_BENCH_WORKER_MATRIX:-1 4 8}"

mkdir -p "$MATRIX_RESULTS"

matrix_index=0
for workers in $WORKER_MATRIX; do
    echo "Running Octane matrix with $workers worker(s)"
    case $((matrix_index % 3)) in
        0) runtime_order="pam frankenphp openswoole" ;;
        1) runtime_order="frankenphp openswoole pam" ;;
        2) runtime_order="openswoole pam frankenphp" ;;
    esac
    PAM_BENCH_WORKERS="$workers" \
    PAM_BENCH_RUNTIME_ORDER="$runtime_order" \
    PAM_BENCH_RESULTS="$MATRIX_RESULTS/workers-$workers" \
        "$ROOT/benchmarks/octane/run.sh"
    matrix_index=$((matrix_index + 1))
done

php "$ROOT/benchmarks/octane/matrix-report.php" "$MATRIX_RESULTS"
php "$ROOT/benchmarks/octane/evidence-manifest.php" "$MATRIX_RESULTS" 2
php "$ROOT/benchmarks/octane/evidence-manifest.php" "$MATRIX_RESULTS" 2 --verify
