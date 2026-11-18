#!/usr/bin/env bash
# The Product debug app on an Android emulator: install, then in light and dark
# a capture, the pixel geometry checks (pam-native-ui tools/geometry.py) and the
# golden comparison (scripts/product-golden.py). Run from the PAM checkout by
# .github/workflows/android-visual.yml; works the same on a local emulator:
#
#   ANDROID_SERIAL=emulator-5554 PAM_PRODUCT_FIXTURE=... PAM_PRODUCT_APK=... \
#   PAM_PRODUCT_PACKAGE=app.pam.productvisual.debug PAM_NATIVE_UI_TOOLS=../pam-native-ui/tools \
#   bash .github/scripts/product-visual-android.sh
#
# RECORD_GOLDENS=1 writes new goldens under the fixture's artifacts instead of comparing.
set -euo pipefail

serial=${ANDROID_SERIAL:-emulator-5554}
package=${PAM_PRODUCT_PACKAGE:?}
fixture=${PAM_PRODUCT_FIXTURE:?}
apk=${PAM_PRODUCT_APK:?}
root=$(pwd)
tools=${PAM_NATIVE_UI_TOOLS:-${root}/pam-native-ui/tools}
goldens=${root}/tests/golden/product/android
shots=${fixture}/apps/native/artifacts/screenshots
artifacts=${fixture}/artifacts
mkdir -p "${shots}" "${artifacts}"

python3 -c 'import PIL' 2>/dev/null \
  || python3 -m pip install --quiet --user --break-system-packages pillow

dev() { adb -s "${serial}" "$@"; }

dev install -r "${apk}"

# The screen is ready when the offline outbox has loaded from native storage.
wait_ready() {
  for _ in $(seq 1 30); do
    if dev shell uiautomator dump /sdcard/pam-product.xml >/dev/null 2>&1 \
      && dev shell cat /sdcard/pam-product.xml | grep -q 'Offline outbox ready.'; then
      dev shell rm -f /sdcard/pam-product.xml
      return 0
    fi
    sleep 1
  done
  echo 'the Product screen did not finish loading' >&2
  return 1
}

failed=0
for mode in light dark; do
  case "${mode}" in light) night=no ;; dark) night=yes ;; esac
  dev shell cmd uimode night "${night}" >/dev/null
  dev shell am force-stop "${package}"
  dev shell monkey -p "${package}" -c android.intent.category.LAUNCHER 1 >/dev/null 2>&1
  wait_ready
  sleep 1
  test -n "$(dev shell pidof "${package}")"
  shot=${shots}/product-native-${mode}.png
  dev exec-out screencap -p >"${shot}"
  python3 "${tools}/geometry.py" --serial "${serial}" --package "${package}" \
    --json "${artifacts}/geometry-${mode}.json" || failed=1
  if [ -n "${RECORD_GOLDENS:-}" ]; then
    python3 scripts/product-golden.py --tools "${tools}" --geometry "${artifacts}/geometry-${mode}.json" \
      --actual "${shot}" --golden "${artifacts}/golden-product-native-${mode}.png" --record
  else
    python3 scripts/product-golden.py --tools "${tools}" --geometry "${artifacts}/geometry-${mode}.json" \
      --actual "${shot}" --golden "${goldens}/product-native-${mode}.png" \
      --diff "${artifacts}/golden-diff-${mode}.png" || failed=1
  fi
done
dev shell cmd uimode night no >/dev/null

sha256sum "${shots}/"*.png >"${artifacts}/android-captures.sha256"
exit "${failed}"
