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
        all|device|simulator)
            slice_selector=$1
            shift
            ;;
        *)
            echo "Usage: $0 [--php 8.4|8.5|EXACT-rN] [all|device|simulator]" >&2
            exit 64
            ;;
    esac
done

for command in python3 curl shasum tar make xcrun xcodebuild cargo rustup lipo; do
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

build_root=$(mktemp -d "${TMPDIR:-/tmp}/pam-php-ios.XXXXXX")
trap 'rm -rf -- "${build_root}"' EXIT
archive=${build_root}/php-${php_version}.tar.xz
curl --fail --location --retry 3 "${php_url}" --output "${archive}"
echo "${php_sha256}  ${archive}" | shasum -a 256 --check
tar -xf "${archive}" -C "${build_root}"
source_root=${build_root}/php-${php_version}
destination=${pam_root}/runtime/ios/${runtime_id}
mkdir -p "${destination}"

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
            -e '/^#define HAVE_POSIX_SPAWN_FILE_ACTIONS_ADDCHDIR_NP 1$/d' \
            -e '/^#define HAVE_POSIX_SPAWN_FILE_ACTIONS_ADDFCHDIR_NP 1$/d' \
            main/php_config.h
        make -j"${PAM_BUILD_JOBS:-$(sysctl -n hw.logicalcpu)}"
        make install
    )
}

build_engine_slice() {
    local target=$1
    rustup target add "${target}"
    cargo build --locked --release --manifest-path "${pam_root}/pam-native/Cargo.toml" \
        -p pam-native-engine --target "${target}"
}

if [[ ${slice_selector} == all || ${slice_selector} == device ]]; then
    build_php_slice device-arm64 iphoneos arm64 aarch64-apple-darwin -miphoneos-version-min
    build_engine_slice aarch64-apple-ios
fi
if [[ ${slice_selector} == all || ${slice_selector} == simulator ]]; then
    build_php_slice simulator-arm64 iphonesimulator arm64 aarch64-apple-darwin -mios-simulator-version-min
    build_php_slice simulator-x86_64 iphonesimulator x86_64 x86_64-apple-darwin -mios-simulator-version-min
    build_engine_slice aarch64-apple-ios-sim
    build_engine_slice x86_64-apple-ios
fi

if [[ ${slice_selector} == all ]]; then
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
else
    echo "Built ${slice_selector} slices. Run with 'all' to create XCFrameworks."
fi
