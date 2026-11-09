#!/usr/bin/env bash
# PAM Octane readiness suite, local runner.
#
#   packages/octane/readiness/run-local.sh [stage...]     stages: suite matrix ecosystem bench
#
# Environment:
#   PAM                   pam binary to test (default: pam on PATH)
#   READINESS_WORK_DIR    disposable work dir (default: $TMPDIR/pam-octane-readiness)
#   READINESS_CLEAN=1     delete apps, vendor dirs, images and containers at the end
#   READINESS_MIGRATE=1   run the matrix in a private network namespace with
#                         net.ipv4.tcp_migrate_req=1 (needs unprivileged user namespaces)
if [ "${READINESS_MIGRATE:-0}" = 1 ] && [ -z "${READINESS_IN_NETNS:-}" ]; then
    # One private network namespace for the whole run (the PAM daemon and
    # every app must share it), with lossless SO_REUSEPORT hand-over.
    exec unshare -rn env READINESS_IN_NETNS=1 bash -c         'ip link set lo up && echo 1 > /proc/sys/net/ipv4/tcp_migrate_req && exec "$0" "$@"' "$0" "$@"
fi
source "$(dirname "$0")/lib.sh"
STAGES=("${@:-suite matrix ecosystem bench}")
read -r -a STAGES <<< "${STAGES[*]}"

# Never touch the user's PAM applications: a private manager state per run.
export PAM_MANAGER_STATE_DIR="$WORK_DIR/pam-state"
export PAM_MANAGER_RUNTIME_DIR="${PAM_MANAGER_RUNTIME_DIR:-/tmp/pam-readiness-$(id -u)}"
mkdir -p "$PAM_MANAGER_STATE_DIR" "$PAM_MANAGER_RUNTIME_DIR"
chmod 700 "$PAM_MANAGER_RUNTIME_DIR"
log "PAM: $("$PAM" --version) ($(command -v "$PAM" || echo "$PAM"))"

cleanup() {
    "$PAM" daemon stop >/dev/null 2>&1 || true
    docker rm -f pam-readiness-redis bench-pam bench-frankenphp bench-swoole bench-roadrunner >/dev/null 2>&1 || true
    if [ "${READINESS_CLEAN:-0}" = 1 ]; then
        log "cleaning $WORK_DIR (results kept in $RESULTS_DIR)"
        find "$WORK_DIR" -mindepth 1 -maxdepth 1 ! -name results -exec rm -rf {} +
        docker images --format '{{.Repository}}:{{.Tag}}' | grep '^pam-octane-bench/' | xargs -r docker rmi >/dev/null 2>&1 || true
    fi
}
trap cleanup EXIT

status=0
for stage in "${STAGES[@]}"; do
    case "$stage" in
        suite) bash "$READINESS_DIR/octane-suite/run.sh" || status=1 ;;
        matrix)
            for version in 12 13; do
                provision_app "$WORK_DIR/l$version" "^$version.0"
                cmd=(python3 "$READINESS_DIR/matrix/matrix.py" --app "$WORK_DIR/l$version" --pam "$PAM"
                     --port "$((8100 + version))" --out "$RESULTS_DIR/matrix-laravel$version.json")
                "${cmd[@]}" || status=1
            done ;;
        ecosystem) bash "$READINESS_DIR/ecosystem/run.sh" || status=1 ;;
        bench) bash "$READINESS_DIR/bench/run.sh" || status=1 ;;
        *) log "unknown stage $stage"; status=64 ;;
    esac
done
python3 "$READINESS_DIR/tools/report.py" "$RESULTS_DIR" > "$RESULTS_DIR/summary.md" || true
log "results: $RESULTS_DIR/summary.md"
exit $status
