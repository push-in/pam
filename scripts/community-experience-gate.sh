#!/usr/bin/env bash
set -euo pipefail

# Release gate for the exact first-run journey documented to the community.
# It intentionally creates every project outside the source checkout.

surface=${1:-all}
pam_bin=${PAM_BIN:-target/debug/pam}
owns_gate_root=0
if [[ -n "${PAM_COMMUNITY_GATE_ROOT:-}" ]]; then
  gate_root=${PAM_COMMUNITY_GATE_ROOT}
  mkdir -p "${gate_root}"
else
  gate_root=$(mktemp -d -t pam-community-gate.XXXXXXXX)
  owns_gate_root=1
fi
keep=${PAM_COMMUNITY_GATE_KEEP:-0}
dev_pid=
active_log=

report_error() {
  local status=$?
  trap - ERR
  local line=${1:-unknown}
  local command=${2:-unknown}
  printf 'Community gate: command failed at line %s (exit %s): %s\n' \
    "${line}" "${status}" "${command}" >&2
  if [[ -n "${active_log}" && -f "${active_log}" ]]; then
    printf '%s\n' '--- pam dev log (last 240 lines) ---' >&2
    tail -240 "${active_log}" >&2 || true
  fi
  return "${status}"
}

trap 'report_error "${LINENO}" "${BASH_COMMAND}"' ERR

if [[ ! -x "${pam_bin}" ]]; then
  printf 'Community gate: PAM_BIN is not executable: %s\n' "${pam_bin}" >&2
  exit 66
fi
pam_bin=$(realpath "${pam_bin}")

stop_dev() {
  if [[ -n "${dev_pid}" ]]; then
    kill "${dev_pid}" 2>/dev/null || true
    wait "${dev_pid}" 2>/dev/null || true
    dev_pid=
  fi
}

cleanup() {
  stop_dev
  if command -v adb >/dev/null 2>&1; then
    adb shell am force-stop dev.pam.communitygate.debug >/dev/null 2>&1 || true
    adb shell am force-stop dev.pam.communityuigate.debug >/dev/null 2>&1 || true
  fi
  if [[ "${keep}" != 1 && "${owns_gate_root}" == 1 && -d "${gate_root}" ]]; then
    local attempt
    for attempt in 1 2 3 4 5; do
      rm -rf -- "${gate_root}" 2>/dev/null || true
      [[ ! -e "${gate_root}" ]] && break
      sleep 1
    done
    if [[ -e "${gate_root}" ]]; then
      printf 'Community gate: could not remove temporary root after stopping pam dev: %s\n' \
        "${gate_root}" >&2
      return 1
    fi
  fi
}
trap cleanup EXIT INT TERM

run_bounded_server_dev() {
  local directory=$1
  local port=$2
  local log=${directory}/community-dev.log
  active_log=${log}
  (
    cd "${directory}"
    exec env PAM_PORT="${port}" "${pam_bin}" dev
  ) >"${log}" 2>&1 &
  dev_pid=$!

  local deadline=$((SECONDS + ${PAM_COMMUNITY_GATE_TIMEOUT_SECONDS:-900}))
  while (( SECONDS < deadline )); do
    if grep -Eiq 'PAM-E[0-9]+|Fatal error|uncaught|dependency resolution failed' "${log}"; then
      printf 'Community gate: server runtime reported an error in %s\n' "${directory}" >&2
      tail -200 "${log}" >&2
      return 1
    fi
    if curl -fsS --connect-timeout 1 --max-time 2 \
      "http://127.0.0.1:${port}/api/ping" | grep -q 'pong'; then
      stop_dev
      active_log=
      return 0
    fi
    if ! kill -0 "${dev_pid}" 2>/dev/null; then
      wait "${dev_pid}" || true
      dev_pid=
      printf 'Community gate: pam dev exited before readiness in %s\n' "${directory}" >&2
      tail -200 "${log}" >&2
      return 1
    fi
    sleep 1
  done
  printf 'Community gate: pam dev did not become ready in %s\n' "${directory}" >&2
  tail -200 "${log}" >&2
  return 1
}

assert_dependency_install() {
  local directory=$1
  if [[ -f "${directory}/composer.json" ]]; then
    if [[ ! -f "${directory}/composer.lock" ]]; then
      printf 'Community gate: composer.lock is missing in Composer project %s\n' "${directory}" >&2
      return 1
    fi
    if [[ ! -f "${directory}/vendor/autoload.php" ]]; then
      printf 'Community gate: Composer dependencies were not installed in %s\n' "${directory}" >&2
      return 1
    fi
    return 0
  fi

  if [[ -f "${directory}/composer.lock" || -d "${directory}/vendor" ]]; then
    printf 'Community gate: dependency artifacts unexpectedly exist in Composer-free project %s\n' "${directory}" >&2
    return 1
  fi
}

init_server() {
  local template=$1
  local name=$2
  local directory=${gate_root}/${name}
  "${pam_bin}" init "${directory}" --template "${template}" --no-interaction
  assert_dependency_install "${directory}"
  run_bounded_server_dev "${directory}" 31987
}

init_mobile() {
  local template=$1
  local name=$2
  local application_id=$3
  local directory=${gate_root}/${name}
  command -v adb >/dev/null 2>&1
  adb get-state >/dev/null
  "${pam_bin}" init "${directory}" --template "${template}" --no-interaction \
    --platform android --application-id "${application_id}" --name "Community Gate"
  assert_dependency_install "${directory}"

  local log=${directory}/community-dev.log
  active_log=${log}
  (
    cd "${directory}"
    exec "${pam_bin}" dev .
  ) >"${log}" 2>&1 &
  dev_pid=$!
  local package=${application_id}.debug
  local deadline=$((SECONDS + ${PAM_COMMUNITY_GATE_TIMEOUT_SECONDS:-1200}))
  while (( SECONDS < deadline )); do
    if adb shell pidof "${package}" 2>/dev/null | grep -Eq '[0-9]'; then
      sleep 3
      local pid
      pid=$(adb shell pidof "${package}" 2>/dev/null | tr -d '\r' || true)
      if [[ -z "${pid}" ]]; then
        printf 'Community gate: %s restarted while collecting evidence; retrying readiness\n' \
          "${package}" >&2
        sleep 2
        continue
      fi
      local logcat_file=${directory}/android-logcat.txt
      adb logcat -d --pid="${pid}" -t 400 >"${logcat_file}" 2>&1 || true
      if grep -Eiq \
        'PluginException|FATAL EXCEPTION|E PamNative.*(error|failed)|Pam Native failed' \
        "${logcat_file}"; then
        printf 'Community gate: native runtime reported an error for %s\n' "${package}" >&2
        tail -400 "${logcat_file}" >&2
        return 1
      fi
      mkdir -p "${directory}/evidence"
      local screenshot=${directory}/evidence/android.png
      local screenshot_ok=0
      local screenshot_attempt
      for screenshot_attempt in 1 2 3 4 5; do
        if adb exec-out screencap -p >"${screenshot}" 2>/dev/null &&
          file "${screenshot}" | grep -q 'PNG image data'; then
          screenshot_ok=1
          break
        fi
        printf 'Community gate: screenshot attempt %s/5 failed for %s; retrying\n' \
          "${screenshot_attempt}" "${package}" >&2
        sleep 2
      done
      if [[ "${screenshot_ok}" != 1 ]]; then
        printf 'Community gate: could not capture valid Android evidence for %s\n' \
          "${package}" >&2
        adb devices -l >&2 || true
        return 1
      fi
      printf 'Community gate: Android launch proven for %s (pid %s, screenshot %s)\n' \
        "${package}" "${pid}" "${screenshot}"
      stop_dev
      active_log=
      return 0
    fi
    if ! kill -0 "${dev_pid}" 2>/dev/null; then
      wait "${dev_pid}" || true
      dev_pid=
      printf 'Community gate: pam dev exited before Android launch for %s\n' "${package}" >&2
      tail -240 "${log}" >&2
      return 1
    fi
    sleep 2
  done
  printf 'Community gate: Android launch timed out for %s\n' "${package}" >&2
  tail -240 "${log}" >&2
  return 1
}

case "${surface}" in
  raw) init_server raw raw-app ;;
  http) init_server http http-app ;;
  laravel) init_server laravel laravel-app ;;
  mobile) init_mobile mobile mobile-app dev.pam.communitygate ;;
  native-ui|mobile-ui) init_mobile native-ui native-ui-app dev.pam.communityuigate ;;
  all)
    init_server raw raw-app
    init_server http http-app
    init_server laravel laravel-app
    init_mobile mobile mobile-app dev.pam.communitygate
    init_mobile native-ui native-ui-app dev.pam.communityuigate
    ;;
  *)
    printf 'Usage: %s [raw|http|laravel|mobile|native-ui|all]\n' "$0" >&2
    exit 64
    ;;
esac

printf 'Community first-run gate passed: %s\n' "${surface}"
