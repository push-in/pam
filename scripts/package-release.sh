#!/usr/bin/env bash

set -euo pipefail

script_directory=$(CDPATH= cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
repository_root=$(CDPATH= cd -- "${script_directory}/.." && pwd)
package_map="${repository_root}/packages/packages.json"

fail() {
    printf 'package-release: %s\n' "$*" >&2
    exit 1
}

require_command() {
    command -v "$1" >/dev/null 2>&1 || fail "required command is unavailable: $1"
}

package_field() {
    local package_name=$1
    local field=$2

    jq -er \
        --arg package_name "${package_name}" \
        --arg field "${field}" \
        '.packages[] | select(.name == $package_name) | .[$field]' \
        "${package_map}" ||
        fail "unknown package or field: ${package_name}.${field}"
}

validate_map() {
    jq -e '
        .owner == "push-in"
        and (.packages | length == 7)
        and ([.packages[].name] | length == (unique | length))
        and ([.packages[].path] | length == (unique | length))
        and ([.packages[].repository] | length == (unique | length))
        and ([.packages[].deploySecret] | length == (unique | length))
        and all(
            .packages[];
            (.name | test("^pushinbr/pam-[a-z0-9-]+$"))
            and (.path | test("^packages/[a-z0-9-]+$"))
            and (.repository | test("^pam-[a-z0-9-]+$"))
            and (.deploySecret | test("^PAM_[A-Z0-9_]+_DEPLOY_KEY$"))
            and (.description | length > 0)
        )
    ' "${package_map}" >/dev/null || fail "packages/packages.json is invalid"
}

validate_packages() {
    require_command composer
    require_command git
    require_command jq
    validate_map

    test -f "${repository_root}/LICENSE" ||
        fail "root Apache 2.0 license is missing"
    test -f "${repository_root}/LICENSING.md" ||
        fail "root licensing guide is missing"
    grep -Fq 'Apache License' "${repository_root}/LICENSE" ||
        fail "root license is not Apache 2.0"
    grep -Fq 'Version 2.0, January 2004' "${repository_root}/LICENSE" ||
        fail "root Apache license version is not 2.0"
    local owner
    owner=$(jq -er '.owner' "${package_map}")

    while IFS=$'\t' read -r package_name package_path repository_name; do
        local directory="${repository_root}/${package_path}"
        local manifest="${directory}/composer.json"

        test -d "${directory}" || fail "package directory is missing: ${package_path}"
        test -f "${manifest}" || fail "composer.json is missing: ${package_path}"
        test -f "${directory}/README.md" || fail "README.md is missing: ${package_path}"
        test -f "${directory}/LICENSE" || fail "LICENSE is missing: ${package_path}"
        cmp -s "${repository_root}/LICENSE" "${directory}/LICENSE" ||
            fail "${package_path}/LICENSE differs from the root Apache license"

        local manifest_name
        manifest_name=$(jq -er '.name' "${manifest}")
        test "${manifest_name}" = "${package_name}" ||
            fail "${package_path}/composer.json declares ${manifest_name}, expected ${package_name}"

        jq -e \
            --arg source "https://github.com/${owner}/${repository_name}" \
            --arg issues "https://github.com/${owner}/pam/issues" \
            '
                (has("version") | not)
                and .license == "Apache-2.0"
                and .type != null
                and .description != null
                and (.keywords | type == "array" and length > 0)
                and .support.source == $source
                and .support.issues == $issues
            ' "${manifest}" >/dev/null ||
            fail "${package_path}/composer.json has incomplete publication metadata"

        composer validate \
            --strict \
            --no-check-lock \
            --no-interaction \
            "${manifest}" >/dev/null

        if git -C "${repository_root}" ls-files "${package_path}" |
            grep -Eq '(^|/)(vendor|node_modules|\.git)(/|$)|(^|/)\.env$'; then
            fail "${package_path} tracks a generated or private path"
        fi
    done < <(
        jq -r '.packages[] | [.name, .path, .repository] | @tsv' "${package_map}"
    )
}

validate_release_tag() {
    local release_tag=$1

    [[ "${release_tag}" =~ ^v([0-9]+)\.([0-9]+)\.([0-9]+)(-[0-9A-Za-z.-]+)?$ ]] ||
        fail "release tag must use SemVer with a v prefix: ${release_tag}"

    local runtime_version
    runtime_version=$(sed -n 's/^version = "\([^"]*\)"$/\1/p' "${repository_root}/Cargo.toml" | head -n 1)
    test -n "${runtime_version}" || fail "unable to read the runtime version from Cargo.toml"
    test "v${runtime_version}" = "${release_tag}" ||
        fail "tag ${release_tag} does not match Cargo.toml version ${runtime_version}"

    local lock_version
    lock_version=$(
        awk '
            $0 == "name = \"pam\"" {
                getline
                value = $0
                sub(/^version = \"/, "", value)
                sub(/\"$/, "", value)
                print value
                exit
            }
        ' "${repository_root}/Cargo.lock"
    )
    test "${lock_version}" = "${runtime_version}" ||
        fail "Cargo.lock PAM version ${lock_version} does not match ${runtime_version}"

    local release_heading="## ${runtime_version} - "
    grep -Eq "^${release_heading}[0-9]{4}-[0-9]{2}-[0-9]{2}$" \
        "${repository_root}/CHANGELOG.md" ||
        fail "CHANGELOG.md does not contain a dated ${runtime_version} release"
    grep -Eq "^${release_heading}[0-9]{4}-[0-9]{2}-[0-9]{2}$" \
        "${repository_root}/packages/octane/CHANGELOG.md" ||
        fail "packages/octane/CHANGELOG.md does not contain a dated ${runtime_version} release"
}

split_package() {
    require_command git

    local package_name=$1
    local source_ref=${2:-HEAD}
    local package_path
    package_path=$(package_field "${package_name}" path)

    git -C "${repository_root}" rev-parse --verify "${source_ref}^{commit}" >/dev/null ||
        fail "source ref does not resolve to a commit: ${source_ref}"

    git -C "${repository_root}" subtree split \
        --prefix="${package_path}" \
        "${source_ref}"
}

verify_split() (
    require_command composer
    require_command git
    require_command jq

    local package_name=$1
    local split_ref=$2
    local expected_files
    expected_files='^(LICENSE|README\.md|CHANGELOG\.md|UPGRADE\.md|CONTRIBUTING\.md|CODE_OF_CONDUCT\.md|SECURITY\.md|PROTOCOL\.md|composer\.json|composer\.lock|config/|docs/|src/|tests/|benchmarks/|resources/|bin/|\.github/|phpstan\.neon|index\.php|phpunit\.xml|\.env\.example|\.gitignore)'

    local temporary_directory
    temporary_directory=$(mktemp -d "${TMPDIR:-/tmp}/pam-package.XXXXXX")
    trap 'rm -rf -- "${temporary_directory}"' EXIT

    git -C "${repository_root}" archive "${split_ref}" | tar -x -C "${temporary_directory}"

    test -f "${temporary_directory}/composer.json" ||
        fail "${package_name} split does not contain composer.json at its root"
    test -f "${temporary_directory}/LICENSE" ||
        fail "${package_name} split does not contain LICENSE at its root"
    test "$(jq -er '.name' "${temporary_directory}/composer.json")" = "${package_name}" ||
        fail "${package_name} split contains the wrong Composer package"

    local unexpected_files
    unexpected_files=$(
        find "${temporary_directory}" -type f -printf '%P\n' |
            grep -Ev "${expected_files}" ||
            true
    )
    if test -n "${unexpected_files}"; then
        printf '%s\n' "${unexpected_files}" >&2
        fail "${package_name} split contains an unexpected file"
    fi

    composer validate \
        --strict \
        --no-check-lock \
        --no-interaction \
        "${temporary_directory}/composer.json" >/dev/null
)

command_name=${1:-}
case "${command_name}" in
    matrix)
        require_command jq
        validate_map
        jq -c '.packages' "${package_map}"
        ;;
    validate)
        validate_packages
        ;;
    validate-tag)
        test "$#" -eq 2 || fail "usage: $0 validate-tag <vX.Y.Z>"
        validate_release_tag "$2"
        ;;
    split)
        test "$#" -ge 2 && test "$#" -le 3 ||
            fail "usage: $0 split <composer-package> [git-ref]"
        split_package "$2" "${3:-HEAD}"
        ;;
    verify-split)
        test "$#" -eq 3 || fail "usage: $0 verify-split <composer-package> <git-ref>"
        verify_split "$2" "$3"
        ;;
    *)
        fail "usage: $0 {matrix|validate|validate-tag|split|verify-split}"
        ;;
esac
