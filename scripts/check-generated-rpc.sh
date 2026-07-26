#!/bin/sh

set -eu

project_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
temporary_root=$(mktemp -d)
trap 'rm -rf -- "$temporary_root"' EXIT

cd "$project_root"

./target/debug/pam contracts \
    tests/fixtures/contracts.php \
    --output "$temporary_root/contracts" >/dev/null
./target/debug/pam rpc generate \
    tests/fixtures/pam.rpc.json \
    --contracts "$temporary_root/contracts/contracts.mobile.json" \
    --output "$temporary_root/rpc" >/dev/null

python3 -m py_compile "$temporary_root/rpc/pam_rpc.py"
rustfmt --check "$temporary_root/rpc/pam_rpc.rs"

if command -v node >/dev/null 2>&1 &&
    node --help 2>&1 | grep -q -- '--experimental-transform-types'; then
    node --experimental-transform-types \
        --check "$temporary_root/rpc/pam-rpc.ts"
fi
