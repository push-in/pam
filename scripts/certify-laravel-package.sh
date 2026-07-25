#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 3 ]]; then
    printf 'Usage: %s vendor/package constraint laravel-major\n' "$0" >&2
    exit 2
fi

package=$1
constraint=$2
laravel=$3
repository=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
report_directory="${PAM_CERTIFICATION_RESULTS:-${repository}/compatibility/results}"
port="${PAM_CERTIFICATION_PORT:-31400}"
workspace=$(mktemp -d)
pam_pid=""
ready=0

cleanup() {
    if [[ -n "${pam_pid}" ]]; then
        kill -TERM "${pam_pid}" 2>/dev/null || true
        wait "${pam_pid}" 2>/dev/null || true
    fi
    if [[ "${PAM_CERTIFICATION_KEEP_WORKSPACE:-false}" == "true" ]]; then
        printf 'Certification workspace kept at %s\n' "${workspace}" >&2
    else
        rm -rf -- "${workspace}"
    fi
}
trap cleanup EXIT

[[ "${package}" =~ ^[a-z0-9_.-]+/[a-z0-9_.-]+$ ]] || {
    printf 'Invalid Composer package name: %s\n' "${package}" >&2
    exit 2
}
[[ "${laravel}" =~ ^(12|13)$ ]] || {
    printf 'Laravel major must be 12 or 13.\n' >&2
    exit 2
}

composer create-project "laravel/laravel:^${laravel}.0" "${workspace}/app" \
    --no-interaction --no-progress --prefer-dist
composer config --working-dir="${workspace}/app" repositories.pam path "${repository}/packages/laravel"

requirements=("pushinbr/pam-laravel:@dev")
if [[ "${package}" != "laravel/framework" ]]; then
    requirements+=("${package}:${constraint}")
fi
composer require --working-dir="${workspace}/app" "${requirements[@]}" \
    --with-all-dependencies --no-interaction --no-progress --prefer-dist

cp "${repository}/compat/certification/pam.php" "${workspace}/app/pam.php"
cp "${repository}/compat/certification/web.php" "${workspace}/app/routes/web.php"
php "${workspace}/app/artisan" package:discover --ansi
php "${workspace}/app/artisan" list --raw | grep -q '^pam:check-production'
php "${workspace}/app/artisan" route:list --json >/dev/null

if [[ ! -x "${repository}/target/release/pam" ]]; then
    cargo build --locked --release --manifest-path "${repository}/Cargo.toml"
fi

(
    cd "${workspace}/app"
    APP_ENV=testing \
    APP_DEBUG=false \
    APP_KEY=base64:MDEyMzQ1Njc4OWFiY2RlZjAxMjM0NTY3ODlhYmNkZWY= \
    DB_CONNECTION=sqlite \
    DB_DATABASE=:memory: \
    CACHE_STORE=array \
    SESSION_DRIVER=array \
    PAM_LARAVEL_OBSERVABILITY=false \
    PAM_STATE_GUARD=true \
    PAM_CERTIFICATION_PORT="${port}" \
        "${repository}/target/release/pam" start pam.php \
        --workers 2 \
        --admin-address "127.0.0.1:$((port + 1000))"
) >"${workspace}/pam.log" 2>&1 &
pam_pid=$!

for _ in $(seq 1 60); do
    if curl --fail --silent "http://127.0.0.1:${port}/api/ping" >/dev/null 2>&1; then
        ready=1
        break
    fi
    if ! kill -0 "${pam_pid}" 2>/dev/null; then
        break
    fi
    sleep 1
done
if [[ "${ready}" -ne 1 ]]; then
    printf 'PAM did not become ready for %s on Laravel %s.\n' "${package}" "${laravel}" >&2
    sed -n '1,200p' "${workspace}/pam.log" >&2
    exit 1
fi
curl --fail --silent "http://127.0.0.1:${port}/api/ping" | grep -q '"pong"'
for _ in $(seq 1 100); do
    curl --fail --silent "http://127.0.0.1:${port}/api/ping" >/dev/null
done

resolved=$(
    composer show --working-dir="${workspace}/app" "${package}" \
        --format=json |
        php -r '$data=json_decode(stream_get_contents(STDIN), true, 512, JSON_THROW_ON_ERROR); echo $data["versions"][0] ?? "unknown";'
)
mkdir -p "${report_directory}"
safe_name=${package//\//-}
php -r '
$report = [
    "schema" => 1,
    "package" => $argv[1],
    "resolved_version" => $argv[2],
    "laravel" => (int) $argv[3],
    "status" => 1,
    "checks" => [
        "composer_install" => true,
        "package_discovery" => true,
        "artisan_boot" => true,
        "route_boot" => true,
        "persistent_requests" => 100,
        "workers" => 2,
    ],
    "certified_at" => gmdate(DATE_ATOM),
];
file_put_contents($argv[4], json_encode($report, JSON_PRETTY_PRINT | JSON_THROW_ON_ERROR | JSON_UNESCAPED_SLASHES).PHP_EOL);
' "${package}" "${resolved}" "${laravel}" "${report_directory}/${safe_name}-laravel-${laravel}.json"
