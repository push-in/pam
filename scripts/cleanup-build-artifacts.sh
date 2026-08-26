#!/usr/bin/env bash
set -euo pipefail

root=${1:-.}
root=$(cd "${root}" && pwd -P)
paths=(
  target
  .pam/cache
  .pam/phpunit-cache
  .pam-native/android/app/build
  .pam-native/android/build
  .pam-native/gradle-home/caches
  .pam-native/gradle-home/daemon
  .pam-native/gradle-home/native
  .pam-native/gradle-home/notifications
  .pam-native/gradle-home/workers
  .pam-native/ios/App/DerivedData
  .pam-native/ios/App/build
  .pam-native/ios/HotReloadBundle
  pam-native/target
  native-sdk/target
)

for relative in "${paths[@]}"; do
  path=${root}/${relative}
  [[ ${path} == "${root}/"* ]] || {
    printf 'refusing cleanup outside %s: %s\n' "${root}" "${path}" >&2
    exit 1
  }
  if [[ -e ${path} || -L ${path} ]]; then
    [[ ! -L ${path} ]] || {
      printf 'refusing symlinked build artifact: %s\n' "${path}" >&2
      exit 1
    }
    find "${path}" -depth -delete
    printf 'cleaned %s\n' "${relative}"
  fi
done
