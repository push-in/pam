#!/usr/bin/env bash
set -euo pipefail

$CREATE_RELEASE()
cd "$FORGE_RELEASE_DIRECTORY"

composer install --no-dev --no-interaction --prefer-dist --optimize-autoloader
pam artisan optimize
pam check-production
pam artisan migrate --force --no-interaction

$ACTIVATE_RELEASE()
$RESTART_QUEUES()

cd "$FORGE_SITE_PATH"
pam restart http
