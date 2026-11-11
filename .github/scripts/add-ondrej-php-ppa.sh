#!/usr/bin/env bash
# Adds ppa:ondrej/php without add-apt-repository.
#
# add-apt-repository asks api.launchpad.net for the PPA's signing key, so a
# Launchpad API outage failed every job that needs libphp8.5-embed even
# though the package archive (ppa.launchpadcontent.net) was still serving.
# The key is vendored and pinned by fingerprint instead.
set -euo pipefail

fingerprint=B8DC7E53946656EFBCE4C1DD71DAEAAB4AD4CAB6
root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
keyring=/usr/share/keyrings/ondrej-php.gpg
list=/etc/apt/sources.list.d/ondrej-php.list
# shellcheck disable=SC1091
codename=$(. /etc/os-release && printf '%s' "${VERSION_CODENAME}")

if [ "$(id -u)" -ne 0 ]; then
    exec sudo -- "$0" "$@"
fi

work=$(mktemp -d)
trap 'rm -rf -- "${work}"' EXIT
gpg --batch --quiet --homedir "${work}" --dearmor \
    --output "${work}/ondrej-php.gpg" "${root}/.github/keys/ondrej-php.asc"
actual=$(gpg --batch --homedir "${work}" --with-colons --show-keys "${work}/ondrej-php.gpg" |
    awk -F: '$1 == "fpr" { print $10; exit }')
if [ "${actual}" != "${fingerprint}" ]; then
    printf 'ondrej/php key fingerprint %s does not match %s\n' "${actual}" "${fingerprint}" >&2
    exit 1
fi
install -D -m 0644 "${work}/ondrej-php.gpg" "${keyring}"

# A copy added earlier (setup-php, the runner image) would duplicate the source.
grep -rlE 'ppa\.launchpad(content)?\.net/ondrej/php' /etc/apt/sources.list.d/ 2>/dev/null |
    grep -vx "${list}" | xargs -r rm -f --
printf 'deb [signed-by=%s] https://ppa.launchpadcontent.net/ondrej/php/ubuntu %s main\n' \
    "${keyring}" "${codename}" >"${list}"

# Transient mirror errors retry instead of failing the job.
printf 'Acquire::Retries "5";\nAcquire::http::Timeout "30";\nAcquire::https::Timeout "30";\n' \
    >/etc/apt/apt.conf.d/80-pam-ci-retries
