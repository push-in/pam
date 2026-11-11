#!/usr/bin/env sh
# Fetches the PHP sources of the PAM runtime (the Pam\* classes that PAM
# compiles into its binary) for PHPStan and php-cli test runs outside PAM.
# Under `pam test` the real runtime is used instead.
set -eu
ref="${PAM_RUNTIME_REF:-v2.2.15}"
dir="$(cd "$(dirname "$0")/../.." && pwd)/.pam-runtime"
mkdir -p "$dir"
for file in bootstrap async native shared laravel; do
    curl --proto '=https' --tlsv1.2 -fsSL \
        "https://raw.githubusercontent.com/push-in/pam/${ref}/runtime/${file}.php" \
        -o "$dir/${file}.php"
done
printf '%s\n' "$ref" > "$dir/REF"
echo "PAM runtime sources ${ref} in ${dir}"
