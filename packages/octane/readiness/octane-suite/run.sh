#!/usr/bin/env bash
# Stage 1: laravel/octane's own test suite on PAM's PHP, then again with every
# Octane request/response routed through the PAM client (PamFakeClient).
source "$(dirname "$0")/../lib.sh"
SUITE_DIR="$WORK_DIR/octane-src"
OUT="$RESULTS_DIR/octane-suite"
mkdir -p "$OUT"

if [ ! -d "$SUITE_DIR/.git" ]; then
    git clone -q https://github.com/laravel/octane "$SUITE_DIR"
fi
(
    cd "$SUITE_DIR"
    git fetch -q --tags
    TAG="${OCTANE_TAG:-$(git describe --tags --abbrev=0 "$(git rev-list --tags --max-count=1)")}"
    git checkout -q -f "$TAG"
    git clean -qfd tests
    echo "$TAG" > "$OUT/octane-tag.txt"
    log "laravel/octane $TAG"
    [ -f vendor/autoload.php ] || "$PAM" composer install --no-interaction --no-progress >/dev/null 2>&1

    # Minimal adapter: the PAM client, its tests, and one hook in TestCase.
    mkdir -p tests/Pam
    cp "$READINESS_DIR/octane-suite/adapter/PamFakeClient.php" tests/Pam/
    cp "$READINESS_DIR/octane-suite/adapter/PamClientTest.php" tests/
    cp "$READINESS_DIR/octane-suite/adapter/bootstrap.php" tests/Pam/
    python3 - "$PACKAGE_DIR" <<'PY'
import json, sys
package = sys.argv[1]
data = json.load(open("composer.json"))
psr4 = data.setdefault("autoload-dev", {}).setdefault("psr-4", {})
psr4["Pam\\Octane\\"] = package + "/src/"
psr4["Laravel\\Octane\\Tests\\Pam\\"] = "tests/Pam/"
json.dump(data, open("composer.json", "w"), indent=4)
case = open("tests/TestCase.php").read()
hook = "getenv('OCTANE_DRIVER') === 'pam' ? new \\Laravel\\Octane\\Tests\\Pam\\PamFakeClient($requests) : new FakeClient($requests)"
case = case.replace("new FakeClient($requests)", hook, 1)
open("tests/TestCase.php", "w").write(case)
PY
    "$PAM" composer dump-autoload -q

    log "upstream suite on PAM's PHP ($("$PAM" -r 'echo PHP_VERSION;'))"
    "$PAM" vendor/bin/phpunit --log-junit "$OUT/upstream.xml" > "$OUT/upstream.txt" 2>&1 || true
    log "upstream suite with requests routed through the PAM client"
    OCTANE_DRIVER=pam PAM_OCTANE_MISMATCH_REPORT="$OUT/pam-crosscheck.json" \
        "$PAM" vendor/bin/phpunit --bootstrap tests/Pam/bootstrap.php --log-junit "$OUT/pam-driver.xml" \
        > "$OUT/pam-driver.txt" 2>&1 || true
)
python3 "$READINESS_DIR/octane-suite/summarize.py" "$OUT"
