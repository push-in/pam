#!/usr/bin/env bash

set -euo pipefail

if (($# != 2)); then
    echo "Usage: package-android-runtime.sh <pam-native-directory> <output-directory>" >&2
    exit 64
fi

script_directory=$(CDPATH= cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
repository_root=$(CDPATH= cd -- "${script_directory}/.." && pwd)
native_root=$(CDPATH= cd -- "$1" && pwd)
output_directory=$2
archive_name=pam-android-runtime.tar.gz

for command_name in jq sha256sum tar; do
    command -v "${command_name}" >/dev/null 2>&1 || {
        echo "Missing required command: ${command_name}" >&2
        exit 1
    }
done

while IFS= read -r runtime_id; do
    for abi in arm64-v8a x86_64; do
        runtime_root="${repository_root}/runtime/android/${runtime_id}/${abi}"
        test -f "${runtime_root}/runtime.json"
        test -f "${runtime_root}/lib/libphp.a"
        test -f "${runtime_root}/include/php/main/php.h"
        test -f "${runtime_root}/include/php/sapi/embed/php_embed.h"
    done
done < <(jq -r '.releases | keys[]' "${repository_root}/runtime/catalog.json")

for target in aarch64-linux-android x86_64-linux-android; do
    test -f "${native_root}/target/${target}/release/libpam_native_engine.a"
done

mkdir -p "${output_directory}"
archive="${output_directory}/${archive_name}"
tar -czf "${archive}" \
    -C "${repository_root}" runtime/catalog.json runtime/android \
    --transform='s,^target/,native/target/,' \
    -C "${native_root}" \
    target/aarch64-linux-android/release/libpam_native_engine.a \
    target/x86_64-linux-android/release/libpam_native_engine.a
(
    cd "${output_directory}"
    sha256sum "${archive_name}" >"${archive_name}.sha256"
)

echo "Packaged verified Android runtimes at ${archive}"
