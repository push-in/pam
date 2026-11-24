#!/usr/bin/env bash
# Backward-compatibility check of packages/nimbus against its last released tag.
#
# Releases are tagged in the split mirror (github.com/push-in/nimbus), not in this
# monorepo, so the check clones the mirror, checks out the newest v* tag, commits the
# current packages/nimbus tree on top of it and runs roave/backward-compatibility-check
# between the two commits.
#
# Usage: scripts/nimbus-bc-check.sh [--from vX.Y.Z] [--format markdown|github-actions|console]
# Env:   NIMBUS_MIRROR (default https://github.com/push-in/nimbus.git)
#        ROAVE_BIN     (default: installs roave/backward-compatibility-check in a temp dir)
set -euo pipefail

repository_root=$(cd "$(dirname "$0")/.." && pwd)
mirror=${NIMBUS_MIRROR:-https://github.com/push-in/nimbus.git}
from=
format=console
while [ "$#" -gt 0 ]; do
    case "$1" in
        --from) from=$2; shift 2 ;;
        --format) format=$2; shift 2 ;;
        *) echo "Unknown option $1" >&2; exit 2 ;;
    esac
done

work=$(mktemp -d)
trap 'rm -rf "${work}"' EXIT

if [ -z "${ROAVE_BIN:-}" ]; then
    mkdir -p "${work}/tools"
    printf '{}\n' >"${work}/tools/composer.json"
    composer --working-dir="${work}/tools" require --no-interaction --quiet 'roave/backward-compatibility-check:^8.27'
    ROAVE_BIN="${work}/tools/vendor/bin/roave-backward-compatibility-check"
fi

git clone --quiet "${mirror}" "${work}/nimbus"
cd "${work}/nimbus"
if [ -z "${from}" ]; then
    from=$(git tag --list 'v*' --sort=-v:refname | head -n 1)
fi
test -n "${from}" || { echo "The mirror has no v* tag to compare against." >&2; exit 1; }
git checkout --quiet -b bc-candidate "${from}"
rsync -a --delete --exclude .git --exclude vendor "${repository_root}/packages/nimbus/" ./
# Accepted 0.x breaks; roave reads the baseline from the working directory.
cp "${repository_root}/scripts/nimbus-bc-baseline.xml" .roave-backward-compatibility-check.xml
git add -A
git -c user.name=bc-check -c user.email=bc-check@localhost commit --quiet --allow-empty -m "packages/nimbus candidate"

echo "Comparing pushinbr/nimbus ${from} with the current packages/nimbus tree."
# The lock file targets the package CI PHP; the analysis only reads sources.
COMPOSER_IGNORE_PLATFORM_REQ=php "${ROAVE_BIN}" \
    --from="${from}" --to=HEAD --install-development-dependencies --format="${format}"
