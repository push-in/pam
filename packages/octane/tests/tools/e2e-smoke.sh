#!/usr/bin/env bash
# End-to-end smoke test of `php artisan octane:start --server=pam` in a fresh
# Laravel application. Meant for a disposable container, for example:
#
#   docker run --rm -v "$PWD:/package:ro" php:8.5-cli \
#       bash /package/tests/tools/e2e-smoke.sh 13
#
# Arguments: the Laravel major version (12 or 13). Env:
#   PAM_OCTANE_VERSION  install this pushinbr/pam-octane version from
#                       Packagist instead of the mounted /package checkout.
#   OCTANE_VERSION      laravel/octane constraint (default ^2.19).
set -euo pipefail
laravel="${1:-13}"
octane="${OCTANE_VERSION:-^2.19}"
package_dir="${PACKAGE_DIR:-/package}"
port=8000
log() { printf '\n==> %s\n' "$*"; }
fail() { printf 'FAIL: %s\n' "$*" >&2; exit 1; }

log "system packages"
if command -v apt-get >/dev/null; then
    export DEBIAN_FRONTEND=noninteractive
    apt-get update -qq >/dev/null && apt-get install -y -qq git unzip curl procps >/dev/null
fi
# Octane's commands handle signals through pcntl (all servers).
if ! php -m | grep -qx pcntl && command -v docker-php-ext-install >/dev/null; then
    docker-php-ext-install pcntl >/dev/null 2>&1
fi
if ! command -v composer >/dev/null; then
    curl -fsSL https://getcomposer.org/installer | php -- --quiet --install-dir=/usr/local/bin --filename=composer
fi
export COMPOSER_ALLOW_SUPERUSER=1 COMPOSER_NO_INTERACTION=1

log "fresh Laravel ${laravel} application"
cd /tmp && rm -rf app
composer create-project --quiet --prefer-dist laravel/laravel app "^${laravel}.0"
cd app
php artisan --version

log "composer require laravel/octane pushinbr/pam-octane"
if [ -n "${PAM_OCTANE_VERSION:-}" ]; then
    composer require --quiet "laravel/octane:${octane}" "pushinbr/pam-octane:${PAM_OCTANE_VERSION}"
else
    cp -r "$package_dir" /tmp/pam-octane-src
    rm -rf /tmp/pam-octane-src/vendor /tmp/pam-octane-src/.pam-runtime /tmp/pam-octane-src/.octane-suite
    composer config repositories.pam-octane '{"type": "path", "url": "/tmp/pam-octane-src", "options": {"symlink": false, "versions": {"pushinbr/pam-octane": "1.99.0"}}}'
    composer require --quiet "laravel/octane:${octane}" "pushinbr/pam-octane:1.99.0"
fi
composer show --direct | grep -E 'octane'

log "php artisan octane:install --server=pam (installs PAM with install.sh)"
php artisan octane:install --server=pam --no-interaction
grep -q '^OCTANE_SERVER=pam' .env || fail "OCTANE_SERVER=pam missing from .env"
export PATH="$HOME/.local/bin:$PATH"
pam --version

log "probe route: tables, octane cache, concurrently, ticks"
cat >> routes/web.php <<'PHP'

Route::get('/probe', function () {
    $table = \Laravel\Octane\Facades\Octane::table('example');
    $table->incr('hits', 'votes');
    $cache = \Illuminate\Support\Facades\Cache::store('octane');

    return [
        'pid' => getmypid(),
        'table' => get_class($table),
        'hits' => $table->get('hits', 'votes'),
        'cache' => $cache->increment('requests'),
        'ticks' => (int) $cache->get('ticks'),
        'tasks' => \Laravel\Octane\Facades\Octane::concurrently([
            fn () => 'task',
            fn () => getmypid(),
        ]),
    ];
});
PHP
php -r '
$file = "app/Providers/AppServiceProvider.php";
$code = file_get_contents($file);
$code = preg_replace("/public function boot\(\): void\s*\{/", "$0\n        \\\\Laravel\\\\Octane\\\\Facades\\\\Octane::tick(\"probe\", fn () => \\\\Illuminate\\\\Support\\\\Facades\\\\Cache::store(\"octane\")->increment(\"ticks\"))->seconds(1);", $code, 1);
file_put_contents($file, $code);
'

log "php artisan octane:start"
php artisan octane:start --port="$port" --workers=2 --task-workers=1 --max-requests=1000 > /tmp/octane.log 2>&1 &
server=$!
for _ in $(seq 1 60); do
    curl -fs "http://127.0.0.1:${port}/up" >/dev/null 2>&1 && break
    sleep 0.5
done
curl -fs "http://127.0.0.1:${port}/up" >/dev/null || { cat /tmp/octane.log; fail "server did not start"; }
php artisan octane:status | grep -q 'Octane server is running' || fail "octane:status"

log "a request"
probe="$(curl -fs "http://127.0.0.1:${port}/probe")"
echo "$probe"
php -r '
$p = json_decode($argv[1], true);
$ok = $p["table"] === "Pam\\Octane\\Tables\\PamTable" && $p["hits"] >= 1 && $p["cache"] >= 1
    && $p["tasks"][0] === "task" && is_int($p["tasks"][1]) && $p["tasks"][1] !== $p["pid"];
exit($ok ? 0 : 1);' "$probe" || fail "unexpected probe response"
sleep 2
ticks="$(curl -fs "http://127.0.0.1:${port}/probe" | php -r 'echo json_decode(stream_get_contents(STDIN), true)["ticks"];')"
[ "$ticks" -ge 1 ] || fail "Octane::tick() did not run (ticks=${ticks})"
echo "ticks: ${ticks}"

log "reload under load, with a new connection per request"
rm -f /tmp/codes.*
load() { local end=$((SECONDS + 12)); while [ "$SECONDS" -lt "$end" ]; do curl -s -o /dev/null -w '%{http_code}\n' "http://127.0.0.1:${port}/up" >> "/tmp/codes.$1" || echo 000 >> "/tmp/codes.$1"; done; }
for i in 1 2 3 4; do load "$i" & done
sleep 2
for _ in 1 2 3; do php artisan octane:reload; sleep 3; done
wait $(jobs -p | grep -v "^${server}$") 2>/dev/null || true
total="$(cat /tmp/codes.* | wc -l)"
failed="$(cat /tmp/codes.* | grep -vc '^200$' || true)"
echo "requests: ${total}, failed: ${failed}"
[ "$total" -gt 100 ] || fail "too few requests (${total})"
[ "$failed" -eq 0 ] || fail "${failed} requests failed during reloads"
curl -fs "http://127.0.0.1:${port}/probe" | grep -q PamTable || fail "probe after reload"
grep -q 'GET /probe' /tmp/octane.log || fail "request log missing from octane:start output"

log "pam artisan octane:status"
pam artisan octane:status | grep -q 'Octane server is running' || fail "pam artisan octane:status"

log "php artisan octane:stop"
php artisan octane:stop
for _ in $(seq 1 30); do kill -0 "$server" 2>/dev/null || break; sleep 0.5; done
kill -0 "$server" 2>/dev/null && fail "octane:start still running after octane:stop"
curl -fs "http://127.0.0.1:${port}/up" >/dev/null 2>&1 && fail "server still answering after octane:stop"
if php artisan octane:status | grep -q 'Octane server is running'; then fail "status after stop"; fi
pgrep -f pam-octane-worker >/dev/null && fail "PAM workers left behind"

log "OK: Laravel ${laravel}, $(composer show laravel/octane | awk '/^versions/{print $NF}'), pam $(pam --version | awk '{print $2}')"
