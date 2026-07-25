#!/bin/sh

set -eu

test "$#" -eq 1 || {
    echo "Usage: verify-packagist-release.sh <vX.Y.Z>" >&2
    exit 64
}

release_tag=$1
release_version=${release_tag#v}
timeout_seconds=${PAM_PACKAGIST_TIMEOUT_SECONDS:-600}
poll_seconds=${PAM_PACKAGIST_POLL_SECONDS:-10}
script_directory=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
repository_root=$(CDPATH= cd -- "${script_directory}/.." && pwd)
package_map="${repository_root}/packages/packages.json"

fail() {
    printf 'packagist-release: %s\n' "$*" >&2
    exit 1
}

require_command() {
    command -v "$1" >/dev/null 2>&1 ||
        fail "required command is unavailable: $1"
}

printf '%s\n' "${release_tag}" |
    grep -Eq '^v[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.-]+)?$' ||
    fail "release tag must use SemVer with a v prefix: ${release_tag}"

for command_name in composer curl date grep jq mktemp sleep tr; do
    require_command "${command_name}"
done

case "${timeout_seconds}:${poll_seconds}" in
    *[!0-9:]*|0:*|*:0)
        fail "poll timeout and interval must be positive integers"
        ;;
esac

temporary_directory=$(mktemp -d "${TMPDIR:-/tmp}/pam-packagist-release.XXXXXX")
cleanup() {
    rm -rf -- "${temporary_directory}"
}
trap cleanup EXIT HUP INT TERM

deadline=$(( $(date +%s) + timeout_seconds ))
poll_attempt=0
while :; do
    poll_attempt=$((poll_attempt + 1))
    unavailable=
    while IFS= read -r package_name; do
        metadata="${temporary_directory}/$(printf '%s' "${package_name}" | tr / _).json"
        metadata_url="https://repo.packagist.org/p2/${package_name}.json"
        metadata_url="${metadata_url}?pam_release=${release_tag}&attempt=${poll_attempt}"
        if ! curl --proto '=https' --tlsv1.2 --fail --silent --show-error \
            --location \
            --header 'User-Agent: PAM-Release-Gate/1.0' \
            --output "${metadata}" \
            "${metadata_url}"; then
            unavailable="${unavailable} ${package_name}"
            continue
        fi
        if ! jq -e \
            --arg package_name "${package_name}" \
            --arg release_tag "${release_tag}" \
            '.packages[$package_name] | any(.version == $release_tag)' \
            "${metadata}" >/dev/null; then
            unavailable="${unavailable} ${package_name}"
        fi
    done <<EOF
$(jq -r '.packages[].name' "${package_map}")
EOF

    while IFS= read -r package_name; do
        metadata="${temporary_directory}/$(printf '%s' "${package_name}" | tr / _).json"
        metadata_url="https://repo.packagist.org/p2/${package_name}.json"
        metadata_url="${metadata_url}?pam_release=${release_tag}&attempt=${poll_attempt}"
        if ! curl --proto '=https' --tlsv1.2 --fail --silent --show-error \
            --location \
            --header 'User-Agent: PAM-Release-Gate/1.0' \
            --output "${metadata}" \
            "${metadata_url}"; then
            unavailable="${unavailable} ${package_name}"
            continue
        fi
        if ! jq -e \
            --arg package_name "${package_name}" \
            '.packages[$package_name] | length > 0' \
            "${metadata}" >/dev/null; then
            unavailable="${unavailable} ${package_name}"
        fi
    done <<EOF
$(jq -r '.runtimePackages[].name' "${package_map}")
EOF

    test -z "${unavailable}" && break
    test "$(date +%s)" -lt "${deadline}" ||
        fail "release ${release_tag} is unavailable for:${unavailable}"
    printf 'Waiting for Packagist:%s\n' "${unavailable}"
    sleep "${poll_seconds}"
done

jq \
    --arg version "${release_version}" \
    '. as $manifest
    | {
        name: "pushinbr/pam-release-gate",
        description: "Temporary public installation gate for a PAM release.",
        type: "project",
        license: "proprietary",
        require: (
            reduce $manifest.packages[].name as $package (
                {php: "^8.4"};
                .[$package] = $version
            )
            | reduce $manifest.runtimePackages[] as $package (
                .;
                .[$package.name] = $package.constraint
            )
        ),
        config: {
            "sort-packages": true
        }
    }' \
    "${package_map}" >"${temporary_directory}/composer.json"

composer validate --strict --no-interaction \
    "${temporary_directory}/composer.json"
composer update --dry-run --no-scripts --no-interaction --prefer-dist \
    --working-dir="${temporary_directory}"
composer update --no-scripts --no-interaction --prefer-dist \
    --working-dir="${temporary_directory}"

while IFS= read -r package_name; do
    installed_version=$(
        composer show --format=json --working-dir="${temporary_directory}" \
            "${package_name}" |
            jq -r '.versions[0]'
    )
    test "${installed_version}" = "${release_tag}" ||
        fail "${package_name} resolved to ${installed_version}, expected ${release_tag}"
done <<EOF
$(jq -r '.packages[].name' "${package_map}")
EOF

while IFS= read -r package_name; do
    composer show --format=json --working-dir="${temporary_directory}" \
        "${package_name}" >/dev/null ||
        fail "${package_name} did not resolve through Packagist"
done <<EOF
$(jq -r '.runtimePackages[].name' "${package_map}")
EOF

printf 'Packagist release %s and all runtime dependencies are publicly installable.\n' \
    "${release_tag}"
