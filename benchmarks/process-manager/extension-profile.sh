#!/usr/bin/env bash
set -euo pipefail

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
results=${PAM_RECOVERY_EXTENSION_RESULTS:-"${root}/benchmarks/process-manager/results/extension-profile"}
rounds=${PAM_RECOVERY_EXTENSION_ROUNDS:-${PAM_RECOVERY_ROUNDS:-10}}
pam_binary=${PAM_BENCH_BINARY:-"${root}/target/release/pam"}

[[ ! -L ${results} ]] || { printf 'refusing symlink extension-profile results directory\n' >&2; exit 1; }
[[ ! -e ${results} ]] || { printf 'refusing to overwrite extension-profile evidence: %s\n' "${results}" >&2; exit 1; }
mkdir -p "${results}"

composer_project=${PAM_RECOVERY_EXTENSION_PROJECT:-"${root}/compat/composer-smoke"}
composer_profile="${results}/composer-extension-profile.json"
"${pam_binary}" extensions "${composer_project}" --no-dev --json >"${composer_profile}"
declarative_config=$(mktemp "${composer_project}/.pam-extension-profile.XXXXXX.toml")
trap 'rm -f -- "${declarative_config}"' EXIT
php -r '
    $profile = json_decode(file_get_contents($argv[1]), true, flags: JSON_THROW_ON_ERROR);
    $extensions = array_map(static fn (string $value): string => json_encode($value, JSON_THROW_ON_ERROR), $profile["selectedExtensions"]);
    printf("schema_version = 1\n\n[applications.evidence]\nkind_code = 1\nscript = \"../../tests/fixtures/server.php\"\ncwd = \".\"\n\n[applications.evidence.php_extension_profile]\nkind_code = 1\nmanifest_sha256 = \"%s\"\nlock_sha256 = \"%s\"\nlock_content_hash = \"%s\"\nextensions = [%s]\n",
        $profile["manifestSha256"], $profile["lockSha256"], $profile["lockContentHash"], implode(", ", $extensions));
' "${composer_profile}" >"${declarative_config}"
"${pam_binary}" config:check "${declarative_config}" --json >"${results}/declarative-profile-check.json"
derived_extensions=$(php -r '
    $profile = json_decode(file_get_contents($argv[1]), true, flags: JSON_THROW_ON_ERROR);
    $extensions = $profile["selectedExtensions"] ?? null;
    if (($profile["schemaVersion"] ?? null) !== 1 || ($profile["stateCode"] ?? null) !== 1
        || ($profile["ready"] ?? null) !== true || ($profile["includeDev"] ?? null) !== false
        || !is_array($extensions) || $extensions === []) { exit(1); }
    foreach ($extensions as $extension) {
        if (!is_string($extension) || preg_match("/^[A-Za-z0-9_-]{1,64}$/D", $extension) !== 1) { exit(1); }
    }
    echo implode(",", $extensions);
' "${composer_profile}")

profile_status=0
for profile in compatible isolated; do
    extensions=
    if [[ ${profile} == isolated ]]; then
        extensions=${derived_extensions}
    fi
    printf 'Measuring 16-worker PAM recovery with %s extension profile\n' "${profile}"
    if ! PAM_RECOVERY_RESULTS="${results}/${profile}" \
    PAM_RECOVERY_ROUNDS="${rounds}" \
    PAM_RECOVERY_WORKERS=16 \
    PAM_RECOVERY_PHP_EXTENSIONS="${extensions}" \
    PAM_RECOVERY_MAX_P95_MILLIS=650 \
    PAM_RECOVERY_MAX_DETECTION_P95_MILLIS=25 \
    PAM_RECOVERY_MAX_BACKOFF_P95_MILLIS=250 \
    PAM_RECOVERY_MAX_READINESS_P95_MILLIS=550 \
    PAM_BENCH_BINARY="${pam_binary}" \
        "${root}/benchmarks/process-manager/run.sh"; then
        profile_status=1
    fi
done

set +e
"${pam_binary}" "${root}/benchmarks/process-manager/extension-profile-report.php" "${results}"
report_status=$?
set -e
"${pam_binary}" "${root}/benchmarks/octane/evidence-manifest.php" "${results}" 8 >/dev/null
"${pam_binary}" "${root}/benchmarks/octane/evidence-manifest.php" "${results}" 8 --verify
(( profile_status == 0 && report_status == 0 ))
