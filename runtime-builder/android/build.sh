#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(CDPATH= cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
PAM_ROOT="$(CDPATH= cd -- "${SCRIPT_DIR}/../.." && pwd)"
CATALOG="${PAM_ROOT}/runtime/catalog.json"
ANDROID_SDK="${ANDROID_HOME:-${ANDROID_SDK_ROOT:-}}"

runtime_selector="${PAM_PHP_VERSION:-8.5}"
abi_selector="all"
while (($# > 0)); do
    case "$1" in
        --php)
            runtime_selector="${2:?--php requires a version such as 8.5}"
            shift 2
            ;;
        all|arm64-v8a|x86_64)
            abi_selector="$1"
            shift
            ;;
        *)
            echo "Usage: $0 [--php 8.4|8.5|EXACT-rN] [all|arm64-v8a|x86_64]" >&2
            exit 64
            ;;
    esac
done

command -v python3 >/dev/null || {
    echo "Required build command is missing: python3" >&2
    exit 1
}
mapfile -t runtime_values < <(python3 - "${CATALOG}" "${runtime_selector}" <<'PY'
import json
import sys

with open(sys.argv[1], encoding="utf-8") as stream:
    catalog = json.load(stream)
selector = sys.argv[2]
release_id = catalog["channels"].get(selector, selector)
try:
    release = catalog["releases"][release_id]
except KeyError:
    raise SystemExit(f"Unknown PHP runtime {selector!r}")
print(release_id)
print(release["phpVersion"])
print(release["runtimeRevision"])
print(release["sourceUrl"])
print(release["sourceSha256"])
print(release["androidApi"])
print(release["ndkVersion"])
print(",".join(release["extensions"]))
PY
)
readonly RUNTIME_ID="${runtime_values[0]}"
readonly PHP_VERSION="${runtime_values[1]}"
readonly RUNTIME_REVISION="${runtime_values[2]}"
readonly PHP_URL="${runtime_values[3]}"
readonly PHP_SHA256="${runtime_values[4]}"
readonly ANDROID_API="${runtime_values[5]}"
readonly NDK_VERSION="${runtime_values[6]}"
readonly PHP_EXTENSIONS="${runtime_values[7]}"
readonly PHP_ARCHIVE="php-${PHP_VERSION}.tar.xz"
opcache_options=()
if [[ "${PHP_VERSION}" == 8.4.* ]]; then
    opcache_options+=(--disable-opcache)
fi

if [[ -z "${ANDROID_SDK}" ]]; then
    echo "ANDROID_HOME must point to an Android SDK installation." >&2
    exit 1
fi

readonly NDK_ROOT="${ANDROID_SDK}/ndk/${NDK_VERSION}"
readonly TOOLCHAIN="${NDK_ROOT}/toolchains/llvm/prebuilt/linux-x86_64"
if [[ ! -x "${TOOLCHAIN}/bin/llvm-ar" ]]; then
    echo "Android NDK ${NDK_VERSION} was not found in ${ANDROID_SDK}." >&2
    exit 1
fi

for command in curl sha256sum tar patch make; do
    command -v "${command}" >/dev/null || {
        echo "Required build command is missing: ${command}" >&2
        exit 1
    }
done

BUILD_ROOT="$(mktemp -d "${TMPDIR:-/tmp}/pam-php-android.XXXXXX")"
trap 'rm -rf -- "${BUILD_ROOT}"' EXIT

curl --fail --location --retry 3 "${PHP_URL}" --output "${BUILD_ROOT}/${PHP_ARCHIVE}"
echo "${PHP_SHA256}  ${BUILD_ROOT}/${PHP_ARCHIVE}" | sha256sum --check --strict
tar -xf "${BUILD_ROOT}/${PHP_ARCHIVE}" -C "${BUILD_ROOT}"

readonly SOURCE="${BUILD_ROOT}/php-${PHP_VERSION}"
patch --directory "${SOURCE}" --strip 1 \
    < "${SCRIPT_DIR}/patches/0001-android-fd-limit.patch"

build_abi() {
    local android_abi="$1"
    local host="$2"
    local clang_prefix="$3"
    local build="${BUILD_ROOT}/build-${android_abi}"
    local install="${BUILD_ROOT}/install-${android_abi}"
        local destination="${PAM_ROOT}/runtime/android/${RUNTIME_ID}/${android_abi}"

    mkdir -p "${build}"
    (
        cd "${build}"
        CC="${TOOLCHAIN}/bin/${clang_prefix}${ANDROID_API}-clang" \
        CXX="${TOOLCHAIN}/bin/${clang_prefix}${ANDROID_API}-clang++" \
        AR="${TOOLCHAIN}/bin/llvm-ar" \
        RANLIB="${TOOLCHAIN}/bin/llvm-ranlib" \
        STRIP="${TOOLCHAIN}/bin/llvm-strip" \
        CFLAGS="-O2 -fPIC -fvisibility=hidden -ffunction-sections -fdata-sections" \
        CPPFLAGS="-D__ANDROID__ -DANDROID" \
        ac_cv_c_bigendian_php=no \
        ac_cv_func_getentropy=no \
        ac_cv_func_arc4random_buf=no \
        ac_cv_func_getrandom=no \
        php_cv_sizeof_intmax_t=8 \
        "${SOURCE}/configure" \
            --build=x86_64-pc-linux-gnu \
            --host="${host}" \
            --prefix="${install}" \
            --with-pic \
            --enable-embed=static \
            --disable-cli \
            --disable-cgi \
            --disable-phpdbg \
            --disable-fpm \
            --disable-all \
            --enable-ctype \
            --enable-filter \
            --enable-session \
            --enable-tokenizer \
            --enable-phar \
            --without-pear \
            --without-iconv \
            --without-libxml \
            --without-openssl \
            --without-zlib \
            --without-curl \
            --without-sqlite3 \
            --without-pdo-sqlite \
            "${opcache_options[@]}" \
            --with-pcre-jit=no

        # PHP's cross checks see part of Android's private resolver surface and
        # register dns_get_* even though the implementation requires APIs that
        # bionic does not expose. Network access is provided by Pam's native
        # HTTP module, so keep those unsupported resolver hooks out entirely.
        sed -i \
            -e '/^#define HAVE_RES_SEARCH 1$/d' \
            -e '/^#define HAVE_RES_NSEARCH 1$/d' \
            -e '/^#define HAVE_DN_SKIPNAME 1$/d' \
            -e '/^#define HAVE_RES_NDESTROY 1$/d' \
            main/php_config.h

        make -j"${PAM_BUILD_JOBS:-$(getconf _NPROCESSORS_ONLN)}"
        make install
    )

    mkdir -p "${destination}/lib"
    rm -rf -- "${destination}/include"
    cp -a "${install}/include" "${destination}/include"
    cp "${install}/lib/libphp.a" "${destination}/lib/libphp.a"
    "${TOOLCHAIN}/bin/llvm-strip" --strip-debug "${destination}/lib/libphp.a"

    python3 - "${destination}/runtime.json" <<PY
import json

manifest = {
    "schemaVersion": 2,
    "runtimeId": "${RUNTIME_ID}",
    "phpVersion": "${PHP_VERSION}",
    "runtimeRevision": ${RUNTIME_REVISION},
    "sourceUrl": "${PHP_URL}",
    "sourceSha256": "${PHP_SHA256}",
    "androidAbi": "${android_abi}",
    "androidApi": ${ANDROID_API},
    "ndkVersion": "${NDK_VERSION}",
    "extensions": "${PHP_EXTENSIONS}".split(","),
}
with open("${destination}/runtime.json", "w", encoding="utf-8") as stream:
    json.dump(manifest, stream, indent=4)
    stream.write("\n")
PY
    echo "Built verified PHP runtime ${RUNTIME_ID} for ${android_abi}."
}

case "${abi_selector}" in
    all)
        build_abi "arm64-v8a" "aarch64-linux-android" "aarch64-linux-android"
        build_abi "x86_64" "x86_64-linux-android" "x86_64-linux-android"
        ;;
    arm64-v8a)
        build_abi "arm64-v8a" "aarch64-linux-android" "aarch64-linux-android"
        ;;
    x86_64)
        build_abi "x86_64" "x86_64-linux-android" "x86_64-linux-android"
        ;;
    *)
        echo "Usage: $0 [--php 8.4|8.5|EXACT-rN] [all|arm64-v8a|x86_64]" >&2
        exit 64
        ;;
esac
