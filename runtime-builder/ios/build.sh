#!/usr/bin/env bash

set -euo pipefail

script_dir=$(CDPATH= cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
pam_root=$(CDPATH= cd -- "${script_dir}/../.." && pwd)
catalog=${pam_root}/runtime/catalog.json
runtime_selector=${PAM_PHP_VERSION:-8.5}
slice_selector=all

while (($# > 0)); do
    case "$1" in
        --php)
            runtime_selector=${2:?--php requires 8.4, 8.5, or an exact runtime id}
            shift 2
            ;;
        all|ios|device|simulator|macos)
            slice_selector=$1
            shift
            ;;
        *)
            echo "Usage: $0 [--php 8.4|8.5|EXACT-rN] [all|ios|device|simulator|macos]" >&2
            exit 64
            ;;
    esac
done

for command in python3 curl shasum tar make perl xcrun xcodebuild cargo rustup lipo; do
    command -v "${command}" >/dev/null || {
        echo "Required iOS build command is missing: ${command}" >&2
        exit 1
    }
done

runtime_values=$(python3 - "${catalog}" "${runtime_selector}" <<'PY'
import json
import sys

with open(sys.argv[1], encoding="utf-8") as stream:
    catalog = json.load(stream)
selector = sys.argv[2]
runtime_id = catalog["channels"].get(selector, selector)
release = catalog["releases"].get(runtime_id)
if release is None:
    raise SystemExit(f"Unknown PHP runtime {selector!r}")
print(runtime_id)
print(release["phpVersion"])
print(release["runtimeRevision"])
print(release["sourceUrl"])
print(release["sourceSha256"])
print(release.get("iosMinimumVersion", "15.0"))
print(",".join(release["extensions"]))
PY
)
runtime_values_array=()
while IFS= read -r value; do
    runtime_values_array+=("${value}")
done <<<"${runtime_values}"
runtime_id=${runtime_values_array[0]}
php_version=${runtime_values_array[1]}
runtime_revision=${runtime_values_array[2]}
php_url=${runtime_values_array[3]}
php_sha256=${runtime_values_array[4]}
ios_minimum=${runtime_values_array[5]}
php_extensions=${runtime_values_array[6]}

# The iOS slices link the PAM Native engine: fail before the long PHP build
# instead of after it when that checkout is missing.
if [[ ${slice_selector} != macos && ! -f ${pam_root}/pam-native/Cargo.toml ]]; then
    echo "PAM Native checkout is missing: expected ${pam_root}/pam-native/Cargo.toml" >&2
    exit 1
fi

build_root=$(mktemp -d "${TMPDIR:-/tmp}/pam-php-ios.XXXXXX")
trap 'rm -rf -- "${build_root}"' EXIT
archive=${build_root}/php-${php_version}.tar.xz
curl --fail --location --retry 3 "${php_url}" --output "${archive}"
echo "${php_sha256}  ${archive}" | shasum -a 256 --check
tar -xf "${archive}" -C "${build_root}"
source_root=${build_root}/php-${php_version}
destination=${pam_root}/runtime/ios/${runtime_id}
mkdir -p "${destination}"

openssl_version=3.5.9
openssl_url=https://github.com/openssl/openssl/releases/download/openssl-${openssl_version}/openssl-${openssl_version}.tar.gz
openssl_sha256=603f5602e2eef00d77fbd429d34dcd5822bb301757a1bc9cdb24c670f1eb859a
openssl_source_root=
macos_openssl_install=

prepare_openssl_source() {
    [[ -n ${openssl_source_root} ]] && return
    local openssl_archive=${build_root}/openssl-${openssl_version}.tar.gz
    curl --fail --location --retry 3 "${openssl_url}" --output "${openssl_archive}"
    echo "${openssl_sha256}  ${openssl_archive}" | shasum -a 256 --check
    tar -xf "${openssl_archive}" -C "${build_root}"
    openssl_source_root=${build_root}/openssl-${openssl_version}
}

build_macos_openssl() {
    local host_arch=$1
    local sdk_path=$2
    local target
    local install=${build_root}/openssl-macos-${host_arch}
    prepare_openssl_source
    case "${host_arch}" in
        arm64) target=darwin64-arm64-cc ;;
        x86_64) target=darwin64-x86_64-cc ;;
        *) echo "Unsupported macOS OpenSSL architecture: ${host_arch}" >&2; return 1 ;;
    esac
    (
        cd "${openssl_source_root}"
        make clean >/dev/null 2>&1 || true
        CC="$(xcrun --sdk macosx --find clang)" \
        CFLAGS="-O2 -fPIC -arch ${host_arch} -isysroot ${sdk_path}" \
        ./Configure "${target}" no-shared no-tests no-module \
            --prefix="${install}" --openssldir="${install}/ssl" --libdir=lib
        make -j"${PAM_BUILD_JOBS:-$(sysctl -n hw.logicalcpu)}"
        make install_sw
    )
    test -f "${install}/lib/libssl.a"
    test -f "${install}/lib/libcrypto.a"
    macos_openssl_install=${install}
}

# iOS forbids the writable/executable memory transitions used by PHP's JIT.
# PHP 8.5 made OPcache part of the core build, so disable JIT on every series
# and disable the optional extension itself only where that switch still exists.
opcache_options=(--disable-opcache-jit)
opcache_enabled=yes
if [[ ${php_version} == 8.4.* ]]; then
    opcache_options=(--disable-opcache --disable-opcache-jit)
    opcache_enabled=no
fi

build_php_slice() {
    local name=$1
    local sdk=$2
    local arch=$3
    local host=$4
    local deployment_flag=$5
    local sdk_path
    local build=${build_root}/build-${name}
    local install=${build_root}/install-${name}
    sdk_path=$(xcrun --sdk "${sdk}" --show-sdk-path)
    mkdir -p "${build}"
    (
        cd "${build}"
        CC="$(xcrun --sdk "${sdk}" --find clang)" \
        CXX="$(xcrun --sdk "${sdk}" --find clang++)" \
        AR="$(xcrun --sdk "${sdk}" --find ar)" \
        RANLIB="$(xcrun --sdk "${sdk}" --find ranlib)" \
        STRIP="$(xcrun --sdk "${sdk}" --find strip)" \
        CFLAGS="-O2 -fPIC -fvisibility=hidden -arch ${arch} -isysroot ${sdk_path} ${deployment_flag}=${ios_minimum}" \
        CPPFLAGS="-arch ${arch} -isysroot ${sdk_path} -D__APPLE__" \
        LDFLAGS="-arch ${arch} -isysroot ${sdk_path} ${deployment_flag}=${ios_minimum}" \
        ac_cv_c_bigendian_php=no \
        ac_cv_func_getentropy=no \
        ac_cv_func_arc4random_buf=no \
        ac_cv_func_getrandom=no \
        php_cv_sizeof_intmax_t=8 \
        php_cv_shm_mmap_anon=${opcache_enabled} \
        php_cv_shm_ipc=no \
        php_cv_shm_mmap_posix=no \
        "${source_root}/configure" \
            --build="$(uname -m)-apple-darwin" \
            --host="${host}" \
            --prefix="${install}" \
            --with-pic \
            --enable-embed=static \
            --disable-cli --disable-cgi --disable-phpdbg --disable-fpm --disable-all \
            --enable-ctype --enable-filter --enable-session --enable-tokenizer --enable-phar \
            --without-pear --without-iconv --without-libxml --without-openssl \
            --without-zlib --without-curl --without-sqlite3 --without-pdo-sqlite \
            "${opcache_options[@]}" --with-pcre-jit=no
        # Darwin headers expose desktop-only spawn helpers while marking them
        # unavailable for iOS. Configure cannot distinguish that availability
        # annotation during a cross build, so remove only those unsupported
        # feature probes before compilation.
        sed -i '' \
            -e '/^#define HAVE_POSIX_SPAWN_FILE_ACTIONS_ADDCHDIR 1$/d' \
            -e '/^#define HAVE_POSIX_SPAWN_FILE_ACTIONS_ADDCHDIR_NP 1$/d' \
            -e '/^#define HAVE_POSIX_SPAWN_FILE_ACTIONS_ADDFCHDIR 1$/d' \
            -e '/^#define HAVE_POSIX_SPAWN_FILE_ACTIONS_ADDFCHDIR_NP 1$/d' \
            main/php_config.h
        if [[ ${name} == simulator-x86_64 ]]; then
            # The distinct iOS host triplet is required to keep Autoconf in
            # cross-compilation mode, but PHP classifies only darwin* as
            # Mach-O when selecting its fiber assembly. Correct that generated
            # build rule without changing target CPU or calling convention.
            sed -i '' \
                's/x86_64_sysv_elf_gas/combined_sysv_macho_gas/g' \
                Makefile Makefile.objects
        fi
        make -j"${PAM_BUILD_JOBS:-$(sysctl -n hw.logicalcpu)}"
        make install
    )
    verify_ios_opcache "${install}" "${name}"
}

# A cross build cannot run OPcache's shared memory probes, so Autoconf used to
# report no SHM model and OPcache refused to start on devices. iOS supports
# mmap(MAP_ANON | MAP_SHARED) (the configure cache values above). Fail the
# build if OPcache, its SHM model or the file cache is missing, or if JIT
# (forbidden by iOS code signing) slipped in.
verify_ios_opcache() {
    local install=$1
    local name=$2
    local config=${install}/include/php/main/php_config.h
    if grep -q '^#define HAVE_JIT 1' "${config}"; then
        echo "iOS ${name} PHP runtime must not enable the OPcache JIT." >&2
        return 1
    fi
    [[ ${opcache_enabled} == yes ]] || return 0
    if ! grep -q '^#define HAVE_SHM_MMAP_ANON 1' "${config}"; then
        echo "iOS ${name} PHP runtime has no OPcache shared memory model." >&2
        return 1
    fi
    # Read the symbol table first: `nm | grep -q` under pipefail fails the
    # check when grep exits early and nm dies of SIGPIPE.
    local symbols
    symbols=$(nm -g "${install}/lib/libphp.a" 2>/dev/null)
    if ! grep -q ' T _zend_file_cache_script_load$' <<<"${symbols}"; then
        echo "iOS ${name} PHP runtime is missing the OPcache file cache." >&2
        return 1
    fi
}

build_engine_slice() {
    local target=$1
    rustup target add "${target}"
    cargo build --locked --release --manifest-path "${pam_root}/pam-native/Cargo.toml" \
        -p pam-native-engine --target "${target}"
}

build_macos_embed_sdk() {
    local host_arch
    local sdk_path
    local build=${build_root}/build-macos-host
    local install=${pam_root}/runtime/macos/${runtime_id}
    local openssl_install
    host_arch=$(uname -m)
    sdk_path=$(xcrun --sdk macosx --show-sdk-path)
    build_macos_openssl "${host_arch}" "${sdk_path}"
    openssl_install=${macos_openssl_install}
    rm -rf -- "${install}"
    mkdir -p "${build}" "${install}"
    (
        cd "${build}"
        if ! CC="$(xcrun --sdk macosx --find clang)" \
        CXX="$(xcrun --sdk macosx --find clang++)" \
        AR="$(xcrun --sdk macosx --find ar)" \
        RANLIB="$(xcrun --sdk macosx --find ranlib)" \
        CFLAGS="-O2 -fPIC -arch ${host_arch} -isysroot ${sdk_path}" \
        CPPFLAGS="-arch ${host_arch} -isysroot ${sdk_path} -I${openssl_install}/include" \
        LDFLAGS="-arch ${host_arch} -isysroot ${sdk_path} -L${openssl_install}/lib" \
        PKG_CONFIG_PATH="${openssl_install}/lib/pkgconfig" \
        OPENSSL_CFLAGS="-I${openssl_install}/include" \
        OPENSSL_LIBS="-L${openssl_install}/lib -lssl -lcrypto" \
        LIBXML_CFLAGS="-I${sdk_path}/usr/include/libxml2" \
        LIBXML_LIBS="-lxml2" \
        XSL_CFLAGS="-I${sdk_path}/usr/include" \
        XSL_LIBS="-lxslt -lxml2" \
        SQLITE_CFLAGS="-I${sdk_path}/usr/include" \
        SQLITE_LIBS="-lsqlite3" \
        ZLIB_CFLAGS="-I${sdk_path}/usr/include" \
        ZLIB_LIBS="-lz" \
        CURL_CFLAGS="-I${sdk_path}/usr/include" \
        CURL_LIBS="-lcurl" \
        "${source_root}/configure" \
            --prefix="${install}" \
            --enable-embed=shared \
            --disable-cli --disable-cgi --disable-phpdbg --disable-fpm --disable-all \
            --enable-ctype --enable-filter --enable-session --enable-tokenizer --enable-phar \
            --enable-mbstring --disable-mbregex --enable-fileinfo --enable-posix --enable-pcntl --enable-sockets \
            --enable-bcmath --enable-calendar --enable-exif --enable-ftp \
            --enable-shmop --enable-sysvmsg --enable-sysvsem --enable-sysvshm \
            --enable-pdo --with-sqlite3 --with-pdo-sqlite \
            --with-libxml --enable-dom --enable-xml --enable-xmlreader --enable-xmlwriter \
            --enable-simplexml --with-xsl \
            --without-pear --with-iconv="${sdk_path}/usr" --with-openssl \
            --with-zlib --with-curl \
            "${opcache_options[@]}" --with-pcre-jit=no; then
            tail -n 120 config.log >&2
            return 1
        fi
        make -j"${PAM_BUILD_JOBS:-$(sysctl -n hw.logicalcpu)}"
        make install
    )
    test -x "${install}/bin/php-config"
    test -f "${install}/lib/libphp.dylib"
    grep -q "^#define HAVE_OPENSSL_EXT 1" "${install}/include/php/main/php_config.h"
    grep -q "^#define HAVE_MBSTRING 1" "${install}/include/php/main/php_config.h"
    xcrun install_name_tool -id @rpath/libphp.dylib "${install}/lib/libphp.dylib"
    python3 - "${install}/runtime.json" <<PY
import json
manifest = {
    "schemaVersion": 1,
    "runtimeId": "${runtime_id}",
    "phpVersion": "${php_version}",
    "runtimeRevision": ${runtime_revision},
    "sourceSha256": "${php_sha256}",
    "platform": "macos",
    "architecture": "${host_arch}",
    "opensslVersion": "${openssl_version}",
    "opensslSourceSha256": "${openssl_sha256}",
    "extensions": ["bcmath", "calendar", "ctype", "curl", "dom", "exif", "fileinfo", "filter", "ftp", "iconv", "mbstring", "openssl", "opcache", "pcntl", "PDO", "pdo_sqlite", "phar", "posix", "session", "shmop", "SimpleXML", "sockets", "sqlite3", "sysvmsg", "sysvsem", "sysvshm", "tokenizer", "uri", "xml", "xmlreader", "xmlwriter", "xsl", "zlib"],
}
with open("${install}/runtime.json", "w", encoding="utf-8") as stream:
    json.dump(manifest, stream, indent=4)
    stream.write("\n")
PY
}

if [[ ${slice_selector} == all || ${slice_selector} == macos ]]; then
    build_macos_embed_sdk
fi
if [[ ${slice_selector} == all || ${slice_selector} == ios || ${slice_selector} == device ]]; then
    build_php_slice device-arm64 iphoneos arm64 aarch64-apple-darwin -miphoneos-version-min
    build_engine_slice aarch64-apple-ios
fi
if [[ ${slice_selector} == all || ${slice_selector} == ios || ${slice_selector} == simulator ]]; then
    build_php_slice simulator-arm64 iphonesimulator arm64 aarch64-apple-darwin -mios-simulator-version-min
    # Keep host distinct from the x86_64 macOS build machine. Otherwise
    # Autoconf treats this as a native build and tries to execute an iOS
    # Simulator probe binary on macOS during configure.
    build_php_slice simulator-x86_64 iphonesimulator x86_64 x86_64-apple-ios -mios-simulator-version-min
    build_engine_slice aarch64-apple-ios-sim
    build_engine_slice x86_64-apple-ios
fi

if [[ ${slice_selector} == all || ${slice_selector} == ios ]]; then
    simulator=${build_root}/simulator
    mkdir -p "${simulator}/php" "${simulator}/engine"
    lipo -create \
        "${build_root}/install-simulator-arm64/lib/libphp.a" \
        "${build_root}/install-simulator-x86_64/lib/libphp.a" \
        -output "${simulator}/php/libphp.a"
    lipo -create \
        "${pam_root}/pam-native/target/aarch64-apple-ios-sim/release/libpam_native_engine.a" \
        "${pam_root}/pam-native/target/x86_64-apple-ios/release/libpam_native_engine.a" \
        -output "${simulator}/engine/libpam_native_engine.a"
    rm -rf -- "${destination}/PamPhp.xcframework" "${destination}/PamNativeEngine.xcframework"
    xcodebuild -create-xcframework \
        -library "${build_root}/install-device-arm64/lib/libphp.a" -headers "${build_root}/install-device-arm64/include" \
        -library "${simulator}/php/libphp.a" -headers "${build_root}/install-simulator-arm64/include" \
        -output "${destination}/PamPhp.xcframework"
    xcodebuild -create-xcframework \
        -library "${pam_root}/pam-native/target/aarch64-apple-ios/release/libpam_native_engine.a" -headers "${pam_root}/pam-native/native" \
        -library "${simulator}/engine/libpam_native_engine.a" -headers "${pam_root}/pam-native/native" \
        -output "${destination}/PamNativeEngine.xcframework"
    python3 - "${destination}/runtime.json" <<PY
import json
manifest = {
    "schemaVersion": 1,
    "runtimeId": "${runtime_id}",
    "phpVersion": "${php_version}",
    "runtimeRevision": ${runtime_revision},
    "sourceUrl": "${php_url}",
    "sourceSha256": "${php_sha256}",
    "iosMinimumVersion": "${ios_minimum}",
    "architectures": ["ios-arm64", "ios-arm64_x86_64-simulator"],
    "extensions": [
        extension
        for extension in "${php_extensions}".split(",")
        if extension != "opcache" or "${opcache_enabled}" == "yes"
    ],
}
with open("${destination}/runtime.json", "w", encoding="utf-8") as stream:
    json.dump(manifest, stream, indent=4)
    stream.write("\n")
PY
    echo "Built verified PAM iOS runtime ${runtime_id} at ${destination}."
elif [[ ${slice_selector} != macos ]]; then
    echo "Built ${slice_selector} slices. Run with 'all' to create XCFrameworks."
else
    echo "Built verified PAM macOS runtime ${runtime_id} at ${pam_root}/runtime/macos/${runtime_id}."
fi
