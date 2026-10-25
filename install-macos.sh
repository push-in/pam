#!/bin/sh

set -eu

if [ "$(uname -s)" != Darwin ]; then
    printf '%s\n' 'pam macOS installer: this entrypoint must run on macOS.' >&2
    exit 1
fi

script_directory=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
exec sh "${script_directory}/install.sh" "$@"
