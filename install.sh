#!/bin/sh

set -eu

repository=${PAM_GITHUB_REPOSITORY:-push-in/pam}
release_api=${PAM_RELEASE_API_URL:-https://api.github.com/repos/${repository}/releases/latest}
release_base=${PAM_RELEASE_BASE_URL:-https://github.com/${repository}/releases/download}
requested_version=${PAM_VERSION:-${1:-}}

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

    case "${source_url}" in
        https://*)
            curl --proto '=https' --tlsv1.2 --fail --silent --show-error \
                --location --output "${destination}" "${source_url}"
            ;;
        *)
            test -n "${PAM_RELEASE_BASE_URL:-}" ||
                fail "refusing a non-HTTPS release URL"
            curl --fail --silent --show-error --location \
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

for command_name in curl tar sha256sum mktemp uname awk find grep sed head readlink; do
    require_command "${command_name}"
done

test "$(uname -s)" = "Linux" || fail "prebuilt PAM releases currently support Linux only"

case "$(uname -m)" in
    x86_64|amd64)
        release_target=linux-x86_64
        ;;
    aarch64|arm64)
        release_target=linux-aarch64
        ;;
    *)
        fail "unsupported architecture: $(uname -m)"
        ;;
esac

if test -z "${requested_version}"; then
    release_metadata=$(mktemp "${TMPDIR:-/tmp}/pam-release.XXXXXX")
    trap 'rm -f -- "${release_metadata}"' EXIT HUP INT TERM
    download "${release_api}" "${release_metadata}"
    requested_version=$(
        sed -n 's/.*"tag_name"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' \
            "${release_metadata}" |
            head -n 1
    )
    rm -f -- "${release_metadata}"
    trap - EXIT HUP INT TERM
fi

printf '%s\n' "${requested_version}" |
    grep -Eq '^v[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.-]+)?$' ||
    fail "release version must use SemVer with a v prefix: ${requested_version}"

archive_name="pam-${requested_version}-${release_target}.tar.gz"
archive_root="pam-${requested_version}-${release_target}"
temporary_directory=$(mktemp -d "${TMPDIR:-/tmp}/pam-install.XXXXXX")
cleanup() {
    rm -rf -- "${temporary_directory}"
}
trap cleanup EXIT HUP INT TERM

download \
    "${release_base}/${requested_version}/${archive_name}" \
    "${temporary_directory}/${archive_name}"
download \
    "${release_base}/${requested_version}/${archive_name}.sha256" \
    "${temporary_directory}/${archive_name}.sha256"

(
    cd "${temporary_directory}"
    sha256sum --check "${archive_name}.sha256"
)

tar -tzf "${temporary_directory}/${archive_name}" |
    awk -v root="${archive_root}" '
        $0 != root && index($0, root "/") != 1 { exit 1 }
        $0 ~ /(^|\/)\.\.(\/|$)/ { exit 1 }
    ' ||
    fail "release archive contains a path outside ${archive_root}"

tar --no-same-owner --no-same-permissions \
    -xzf "${temporary_directory}/${archive_name}" \
    -C "${temporary_directory}"

if find "${temporary_directory}/${archive_root}" -type l -print -quit | grep -q .; then
    fail "release archive contains an unexpected symbolic link"
fi

data_home=${XDG_DATA_HOME:-${HOME:?HOME is required}/.local/share}
binary_home=${XDG_BIN_HOME:-${HOME:?HOME is required}/.local/bin}
install_root=${PAM_INSTALL_DIR:-${data_home}/pam}
binary_directory=${PAM_BIN_DIR:-${binary_home}}
release_directory="${install_root}/${requested_version}-${release_target}"

mkdir -p "${install_root}" "${binary_directory}"
if test -e "${release_directory}"; then
    test -x "${release_directory}/bin/pam-run" ||
        fail "existing installation is incomplete: ${release_directory}"
else
    mv "${temporary_directory}/${archive_root}" "${release_directory}"
fi

binary_link="${binary_directory}/pam"
if test -e "${binary_link}" && test ! -L "${binary_link}"; then
    fail "refusing to replace a non-symlink: ${binary_link}"
fi
ln -sfn "${release_directory}/bin/pam-run" "${binary_link}"

"${binary_link}" --version
printf 'PAM installed at %s\n' "${release_directory}"
case ":${PATH}:" in
    *":${binary_directory}:"*)
        ;;
    *)
        printf 'Add %s to PATH to run pam directly.\n' "${binary_directory}"
        ;;
esac
