# shellcheck shell=bash
# Shared helpers for the PAM Octane readiness suite.
set -euo pipefail

READINESS_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PACKAGE_DIR="$(cd "$READINESS_DIR/.." && pwd)"
WORK_DIR="${READINESS_WORK_DIR:-${TMPDIR:-/tmp}/pam-octane-readiness}"
RESULTS_DIR="${READINESS_RESULTS_DIR:-$WORK_DIR/results}"
PAM="${PAM:-pam}"
mkdir -p "$WORK_DIR" "$RESULTS_DIR"

log() { printf '\033[1m==> %s\033[0m\n' "$*" >&2; }

# provision_app <dir> <laravel-constraint> [package-source: path|packagist]
# Creates a disposable Laravel app wired to laravel/octane + pushinbr/pam-octane
# and the readiness probe routes.
provision_app() {
    local dir="$1" constraint="$2" source="${3:-path}"
    if [ -f "$dir/vendor/autoload.php" ] && [ -f "$dir/routes/readiness.php" ]; then
        return 0
    fi
    rm -rf "$dir"
    log "creating Laravel $constraint app in $dir"
    "$PAM" composer create-project --no-interaction --prefer-dist "laravel/laravel:$constraint" "$dir" >/dev/null
    (
        cd "$dir"
        if [ "$source" = path ]; then
            "$PAM" composer config repositories.pam-octane \
                "{\"type\":\"path\",\"url\":\"$PACKAGE_DIR\",\"options\":{\"symlink\":false,\"versions\":{\"pushinbr/pam-octane\":\"1.99.0\"}}}"
            "$PAM" composer require --no-interaction -W laravel/octane "pushinbr/pam-octane:1.99.0" >/dev/null
        else
            "$PAM" composer require --no-interaction -W laravel/octane pushinbr/pam-octane >/dev/null
        fi
        cp -R "$READINESS_DIR/matrix/overlay/." .
        # Probe routes are registered without the web group so stateless
        # endpoints are not subject to sessions/CSRF; isolation routes opt in.
        php_patch='s#health: '"'"'/up'"'"',#health: '"'"'/up'"'"',\n        then: fn () => \\Illuminate\\Support\\Facades\\Route::group([], base_path('"'"'routes/readiness.php'"'"')),#'
        sed -i "$php_patch" bootstrap/app.php
        sed -i 's#->withMiddleware(function (Middleware $middleware): void {#->withMiddleware(function (Middleware $middleware): void {\n        $middleware->trustProxies(at: '"'"'*'"'"');#' bootstrap/app.php
        sed -i 's/^SESSION_DRIVER=.*/SESSION_DRIVER=file/; s/^CACHE_STORE=.*/CACHE_STORE=file/; s/^APP_DEBUG=.*/APP_DEBUG=false/; s/^LOG_LEVEL=.*/LOG_LEVEL=warning/' .env
        touch database/database.sqlite
        "$PAM" artisan migrate --force >/dev/null
        "$PAM" artisan tinker --execute='App\Models\User::factory()->count(100)->create();' >/dev/null
        "$PAM" artisan route:list --path=_r >/dev/null
    )
}

# wait_http <url> [timeout-seconds]
wait_http() {
    local url="$1" deadline=$((SECONDS + ${2:-60}))
    until curl -fsS -o /dev/null "$url" 2>/dev/null; do
        if [ $SECONDS -ge $deadline ]; then
            log "timeout waiting for $url"
            return 1
        fi
        sleep 0.3
    done
}
