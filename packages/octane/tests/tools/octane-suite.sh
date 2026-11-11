#!/usr/bin/env bash
# Runs laravel/octane's own test suite with this package installed: every
# Octane application also registers PamOctaneServiceProvider. Three runs:
#   1. upstream suite on PAM's PHP (plus Octane's Swoole-only table and
#      cache store tests ported to PAM tables: tests/OctaneSuite);
#   2. the same suite with every request and response routed through the
#      real PAM client (PamFakeClient), cross-checked against Laravel;
#   3. the upstream suite on the host php-cli, when one is available.
# Usage: tests/tools/octane-suite.sh [octane-tag]   (default: the latest tag)
# Env: PAM (the pam binary), OCTANE_SUITE_DIR (work directory).
set -euo pipefail
package="$(cd "$(dirname "$0")/../.." && pwd)"
pam="${PAM:-pam}"
work="${OCTANE_SUITE_DIR:-$package/.octane-suite}"
mkdir -p "$work"
if [ ! -d "$work/octane/.git" ]; then
    git clone -q https://github.com/laravel/octane "$work/octane"
fi
cd "$work/octane"
git fetch -q --tags
tag="${1:-$(git describe --tags --abbrev=0 "$(git rev-list --tags --max-count=1)")}"
git checkout -q -f "$tag"
git clean -qfdx tests
echo "laravel/octane $tag"

mkdir -p tests/Pam
cp "$package"/tests/OctaneSuite/PamFakeClient.php "$package"/tests/OctaneSuite/bootstrap.php tests/Pam/
cp "$package"/tests/OctaneSuite/PamTableTest.php "$package"/tests/OctaneSuite/PamOctaneStoreTest.php tests/
[ -f tests/PamClientTest.php ] || cp "$package"/tests/OctaneSuite/PamClientTest.php tests/
python3 - "$package" <<'PY'
import json, sys
package = sys.argv[1]
data = json.load(open("composer.json"))
psr4 = data.setdefault("autoload-dev", {}).setdefault("psr-4", {})
psr4["Pam\\Octane\\"] = package + "/src/"
psr4["Laravel\\Octane\\Tests\\Pam\\"] = "tests/Pam/"
data.setdefault("require-dev", {})["pushinbr/pam-contracts"] = "^1.1"
json.dump(data, open("composer.json", "w"), indent=4)
case = open("tests/TestCase.php").read()
for old, new in [
    ("new FakeClient($requests)",
     "getenv('OCTANE_DRIVER') === 'pam' ? new \\Laravel\\Octane\\Tests\\Pam\\PamFakeClient($requests) : new FakeClient($requests)"),
    ("        $app->register(new OctaneServiceProvider($app));\n",
     "        $app->register(new OctaneServiceProvider($app));\n        $app->register(new \\Pam\\Octane\\PamOctaneServiceProvider($app));\n"),
    ("        $factory->warm($app, Octane::defaultServicesToWarm());\n",
     "        $app->register(new \\Pam\\Octane\\PamOctaneServiceProvider($app));\n\n        $factory->warm($app, Octane::defaultServicesToWarm());\n"),
]:
    if old not in case:
        sys.exit("unexpected tests/TestCase.php: cannot find " + old.strip())
    case = case.replace(old, new, 1)
open("tests/TestCase.php", "w").write(case)
PY
git checkout -q -- composer.lock 2>/dev/null || true
"$pam" composer update --no-interaction --no-progress --prefer-dist -q

status=0
echo "1. upstream suite with pam-octane on PAM's PHP"
"$pam" vendor/bin/phpunit || status=1
echo "2. the same suite through the PAM client"
OCTANE_DRIVER=pam PAM_OCTANE_MISMATCH_REPORT="$work/crosscheck.json" \
    "$pam" vendor/bin/phpunit --bootstrap tests/Pam/bootstrap.php || status=1
python3 - "$work/crosscheck.json" <<'PY' || status=1
import json, sys
report = json.load(open(sys.argv[1]))
exchanges = report["exchanges"]
count = len(exchanges) if isinstance(exchanges, list) else exchanges
print(f"cross-checked exchanges: {count}, mismatches: {len(report['mismatches'])}")
sys.exit(1 if report["mismatches"] or not count else 0)
PY
if command -v php >/dev/null 2>&1; then
    echo "3. upstream suite with pam-octane on $(php -r 'echo PHP_VERSION;')"
    php vendor/bin/phpunit --exclude-filter 'Pam' || status=1
fi
exit "$status"
