#!/bin/sh

set -eu

repository=${PAM_GITHUB_REPOSITORY:-push-in/pam}
release_api=${PAM_RELEASE_API_URL:-https://api.github.com/repos/${repository}/releases/latest}
release_base=${PAM_RELEASE_BASE_URL:-https://github.com/${repository}/releases/download}
requested_version=${PAM_VERSION:-${1:-}}
release_metadata_max_bytes=1048576
release_archive_max_bytes=1073741824
release_checksum_max_bytes=16384
release_extracted_max_bytes=4294967296
release_extracted_max_entries=100000
release_retained_previous=2

fail() {
    printf 'pam installer: %s\n' "$*" >&2
    exit 1
}

require_command() {
    command -v "$1" >/dev/null 2>&1 ||
        fail "required command is unavailable: $1"
}

download() {
    source_url=$1
    destination=$2
    maximum_bytes=$3

    case "${source_url}" in
        https://*)
            curl --proto '=https' --proto-redir '=https' --tlsv1.2 \
                --connect-timeout 15 --max-time 600 \
                --max-filesize "${maximum_bytes}" --fail --silent --show-error \
                --location --output "${destination}" "${source_url}"
            ;;
        *)
            test -n "${PAM_RELEASE_BASE_URL:-}" ||
                fail "refusing a non-HTTPS release URL"
            curl --proto-redir '=http,https' --connect-timeout 15 --max-time 600 \
                --max-filesize "${maximum_bytes}" --fail --silent --show-error --location \
                --output "${destination}" "${source_url}"
            ;;
    esac
}

case "${requested_version}" in
    -h|--help)
        cat <<'EOF'
Usage: install.sh [vX.Y.Z]

Environment:
  PAM_VERSION             Release tag; defaults to the latest GitHub release
  PAM_INSTALL_DIR         Versioned runtime directory
  PAM_BIN_DIR             Directory for the pam symlink
  PAM_RELEASE_BASE_URL    Alternate release base URL for controlled mirrors
EOF
        exit 0
        ;;
esac

for command_name in curl tar mktemp uname awk find grep sed head readlink wc; do
    require_command "${command_name}"
done

case "$(uname -s):$(uname -m)" in
    Linux:x86_64|Linux:amd64)
        release_target=linux-x86_64
        checksum_command=sha256sum
        ;;
    Linux:aarch64|Linux:arm64)
        release_target=linux-aarch64
        checksum_command=sha256sum
        ;;
    Darwin:arm64|Darwin:aarch64)
        release_target=macos-arm64
        checksum_command=shasum
        ;;
    Darwin:x86_64|Darwin:amd64)
        release_target=macos-x86_64
        checksum_command=shasum
        ;;
    *)
        fail "unsupported platform: $(uname -s) $(uname -m)"
        ;;
esac
require_command "${checksum_command}"

if test -z "${requested_version}"; then
    release_metadata=$(mktemp "${TMPDIR:-/tmp}/pam-release.XXXXXX")
    trap 'rm -f -- "${release_metadata}"' EXIT HUP INT TERM
    download "${release_api}" "${release_metadata}" "${release_metadata_max_bytes}"
    requested_version=$(
        sed -n 's/.*"tag_name"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' \
            "${release_metadata}" |
            head -n 1
    )
    rm -f -- "${release_metadata}"
    trap - EXIT HUP INT TERM
fi

printf '%s\n' "${requested_version}" | awk '
    function number(value) {
        return value ~ /^(0|[1-9][0-9]*)$/
    }
    {
        if (substr($0, 1, 1) != "v" || index($0, "+") != 0) exit 1
        version = substr($0, 2)
        separator = index(version, "-")
        core = separator ? substr(version, 1, separator - 1) : version
        prerelease = separator ? substr(version, separator + 1) : ""
        if (split(core, parts, ".") != 3 ||
            !number(parts[1]) || !number(parts[2]) || !number(parts[3])) exit 1
        if (separator) {
            identifier_count = split(prerelease, identifiers, ".")
            if (prerelease == "" || identifier_count == 0) exit 1
            for (position = 1; position <= identifier_count; position++) {
                identifier = identifiers[position]
                if (identifier == "" || identifier !~ /^[0-9A-Za-z-]+$/) exit 1
                if (identifier ~ /^[0-9]+$/ && !number(identifier)) exit 1
            }
        }
    }
' ||
    fail "release version must use SemVer with a v prefix: ${requested_version}"

archive_name="pam-${requested_version}-${release_target}.tar.gz"
archive_root="pam-${requested_version}-${release_target}"
temporary_directory=$(mktemp -d "${TMPDIR:-/tmp}/pam-install.XXXXXX")
activation_link=
new_release_directory=
release_stage=
cleanup() {
    rm -rf -- "${temporary_directory}"
    if test -n "${activation_link}"; then
        rm -f -- "${activation_link}"
    fi
    if test -n "${new_release_directory}"; then
        rm -rf -- "${new_release_directory}"
    fi
    if test -n "${release_stage}"; then
        rm -rf -- "${release_stage}"
    fi
}
trap cleanup EXIT HUP INT TERM

prune_old_releases() {
    previous_count=0
    for candidate_release in "${install_root}"/v*-"${release_target}"; do
        test "${candidate_release}" != "${release_directory}" || continue
        test ! -L "${candidate_release}" && test -d "${candidate_release}" &&
            test -x "${candidate_release}/bin/pam-run" || continue
        previous_count=$((previous_count + 1))
    done

    while test "${previous_count}" -gt "${release_retained_previous}"; do
        oldest_release=
        for candidate_release in "${install_root}"/v*-"${release_target}"; do
            test "${candidate_release}" != "${release_directory}" || continue
            test ! -L "${candidate_release}" && test -d "${candidate_release}" &&
                test -x "${candidate_release}/bin/pam-run" || continue
            if test -z "${oldest_release}" || test "${candidate_release}" -ot "${oldest_release}"; then
                oldest_release=${candidate_release}
            fi
        done
        test -n "${oldest_release}" || return 1
        rm -rf -- "${oldest_release}" || return 1
        previous_count=$((previous_count - 1))
    done
}

probe_runtime_identity() {
    candidate_binary=$1
    identity_output="${temporary_directory}/candidate-identity.txt"
    : >"${identity_output}"
    (
        ulimit -t 5
        ulimit -f 4
        exec "${candidate_binary}" --version
    ) >"${identity_output}" 2>&1 &
    identity_pid=$!
    (
        identity_sleep=
        trap '
            if test -n "${identity_sleep}"; then
                kill "${identity_sleep}" 2>/dev/null || :
                wait "${identity_sleep}" 2>/dev/null || :
            fi
            exit 0
        ' HUP INT TERM
        sleep 5 &
        identity_sleep=$!
        wait "${identity_sleep}" || exit 0
        identity_sleep=
        if kill -0 "${identity_pid}" 2>/dev/null; then
            kill -TERM "${identity_pid}" 2>/dev/null || :
            sleep 1 &
            identity_sleep=$!
            wait "${identity_sleep}" || exit 0
            identity_sleep=
            kill -KILL "${identity_pid}" 2>/dev/null || :
        fi
    ) &
    identity_watchdog=$!
    identity_status=0
    wait "${identity_pid}" || identity_status=$?
    kill "${identity_watchdog}" 2>/dev/null || :
    wait "${identity_watchdog}" 2>/dev/null || :
    test "${identity_status}" = 0 || return 1

    identity_bytes=$(wc -c <"${identity_output}")
    identity_lines=$(awk 'END { print NR }' "${identity_output}")
    test "${identity_bytes}" -gt 0 && test "${identity_bytes}" -le 4096 || return 1
    test "${identity_lines}" = 1 || return 1
    installed_identity=$(sed -n '1p' "${identity_output}")
}

download \
    "${release_base}/${requested_version}/${archive_name}" \
    "${temporary_directory}/${archive_name}" \
    "${release_archive_max_bytes}"
download \
    "${release_base}/${requested_version}/${archive_name}.sha256" \
    "${temporary_directory}/${archive_name}.sha256" \
    "${release_checksum_max_bytes}"

checksum_line=$(sed -n '1p' "${temporary_directory}/${archive_name}.sha256")
checksum_lines=$(awk 'END { print NR }' "${temporary_directory}/${archive_name}.sha256")
expected_checksum=$(printf '%s\n' "${checksum_line}" | awk -v archive="${archive_name}" '
    length($1) == 64 && $1 ~ /^[0-9a-f]+$/ &&
        ($2 == archive || $2 == "*" archive) && NF == 2 { print $1 }
')
test "${checksum_lines}" = 1 && test -n "${expected_checksum}" ||
    fail "release checksum must contain exactly one SHA-256 entry for ${archive_name}"
if test -n "${PAM_EXPECTED_ARCHIVE_SHA256:-}"; then
    test "${PAM_EXPECTED_ARCHIVE_SHA256}" = "${expected_checksum}" ||
        fail "release checksum does not match the signed update authorization"
fi

if test "${checksum_command}" = shasum; then
    actual_checksum=$(shasum -a 256 "${temporary_directory}/${archive_name}" | awk '{ print $1 }')
else
    actual_checksum=$(sha256sum "${temporary_directory}/${archive_name}" | awk '{ print $1 }')
fi
test "${actual_checksum}" = "${expected_checksum}" ||
    fail "release archive checksum mismatch"

tar -tzf "${temporary_directory}/${archive_name}" |
    awk -v root="${archive_root}" '
        $0 != root && index($0, root "/") != 1 { exit 1 }
        $0 ~ /(^|\/)\.\.(\/|$)/ { exit 1 }
    ' ||
    fail "release archive contains a path outside ${archive_root}"

(
    ulimit -t 900
    ulimit -f 4194304
    tar --no-same-owner --no-same-permissions \
        -xzf "${temporary_directory}/${archive_name}" \
        -C "${temporary_directory}"
) || fail "release archive extraction exceeded its resource limits"

if find "${temporary_directory}/${archive_root}" \
    ! -type f ! -type d -print -quit | grep -q .; then
    fail "release archive contains an unexpected special file"
fi
if find "${temporary_directory}/${archive_root}" \
    -type f -links +1 -print -quit | grep -q .; then
    fail "release archive contains an unexpected hard link"
fi

extracted_entries=$(find "${temporary_directory}/${archive_root}" -print |
    awk 'END { print NR }')
test "${extracted_entries}" -le "${release_extracted_max_entries}" ||
    fail "release archive expands to too many entries: ${extracted_entries}"
extracted_bytes=$(
    find "${temporary_directory}/${archive_root}" -type f \
        -exec sh -c 'for path do wc -c < "${path}"; done' sh {} + |
        awk '{ total += $1 } END { printf "%.0f\n", total }'
)
test "${extracted_bytes}" -le "${release_extracted_max_bytes}" ||
    fail "release archive expands beyond four GiB: ${extracted_bytes} bytes"

data_home=${XDG_DATA_HOME:-${HOME:?HOME is required}/.local/share}
binary_home=${XDG_BIN_HOME:-${HOME:?HOME is required}/.local/bin}
install_root=${PAM_INSTALL_DIR:-${data_home}/pam}
binary_directory=${PAM_BIN_DIR:-${binary_home}}
release_directory="${install_root}/${requested_version}-${release_target}"

mkdir -p "${install_root}" "${binary_directory}"
if test -e "${release_directory}"; then
    test ! -L "${release_directory}" && test -d "${release_directory}" &&
        test -x "${release_directory}/bin/pam-run" ||
        fail "existing installation is incomplete: ${release_directory}"
    candidate_directory=${release_directory}
else
    release_stage_candidate="${release_directory}.installing"
    mkdir "${release_stage_candidate}" ||
        fail "another installation is active or stale: ${release_stage_candidate}"
    release_stage=${release_stage_candidate}
    mv "${temporary_directory}/${archive_root}" "${release_stage}/runtime"
    candidate_directory="${release_stage}/runtime"
fi

probe_runtime_identity "${candidate_directory}/bin/pam-run" ||
    fail "installed runtime did not report its identity"
expected_identity="pam ${requested_version#v}"
test "${installed_identity}" = "${expected_identity}" ||
    fail "runtime identity mismatch: expected ${expected_identity}, received ${installed_identity}"

if test -n "${release_stage}"; then
    test ! -e "${release_directory}" && test ! -L "${release_directory}" ||
        fail "release destination appeared during installation: ${release_directory}"
    mv "${candidate_directory}" "${release_directory}"
    new_release_directory=${release_directory}
    rmdir "${release_stage}"
    release_stage=
fi

binary_link="${binary_directory}/pam"
if test -e "${binary_link}" && test ! -L "${binary_link}"; then
    fail "refusing to replace a non-symlink: ${binary_link}"
fi
activation_link="${binary_link}.next.$$.tmp"
test ! -e "${activation_link}" && test ! -L "${activation_link}" ||
    fail "refusing an existing activation path: ${activation_link}"
ln -s "${release_directory}/bin/pam-run" "${activation_link}"
mv -f "${activation_link}" "${binary_link}"
activation_link=
new_release_directory=

if ! prune_old_releases; then
    printf 'pam installer: warning: could not prune older PAM releases\n' >&2
fi

printf '%s\n' "${installed_identity}"
printf 'PAM installed at %s\n' "${release_directory}"
case ":${PATH}:" in
    *":${binary_directory}:"*)
        ;;
    *)
        printf 'Add %s to PATH to run pam directly.\n' "${binary_directory}"
        ;;
esac
