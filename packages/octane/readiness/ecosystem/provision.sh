#!/usr/bin/env bash
# Stage 3 app: Laravel 12 + Octane + Livewire 3 + Inertia (Vue) + Filament 4 +
# Sanctum + Horizon + Reverb + queues + scheduler + storage + mail(log).
source "$(dirname "$0")/../lib.sh"
APP="${ECO_APP:-$WORK_DIR/eco}"
if [ -f "$APP/.readiness-provisioned" ]; then exit 0; fi
rm -rf "$APP"
log "creating ecosystem app in $APP"
"$PAM" composer create-project --no-interaction --prefer-dist "laravel/laravel:^12.0" "$APP" >/dev/null 2>&1
cd "$APP"
"$PAM" composer config repositories.pam-octane \
    "{\"type\":\"path\",\"url\":\"$PACKAGE_DIR\",\"options\":{\"symlink\":false,\"versions\":{\"pushinbr/pam-octane\":\"1.99.0\"}}}"
"$PAM" composer require --no-interaction -W laravel/octane pushinbr/pam-octane:1.99.0 \
    'livewire/livewire:^3' 'inertiajs/inertia-laravel:^2' 'filament/filament:^4' 'laravel/sanctum:^4' \
    'laravel/horizon:^5' 'laravel/reverb:^1' 'predis/predis:^2' >/dev/null 2>&1
cat >> .env <<'ENV'
REVERB_APP_ID=100001
REVERB_APP_KEY=readinesskey
REVERB_APP_SECRET=readinesssecret
REVERB_HOST=127.0.0.1
REVERB_PORT=8390
REVERB_SCHEME=http
REVERB_SERVER_PORT=8390
VITE_REVERB_APP_KEY="${REVERB_APP_KEY}"
VITE_REVERB_HOST="${REVERB_HOST}"
VITE_REVERB_PORT="${REVERB_PORT}"
VITE_REVERB_SCHEME="${REVERB_SCHEME}"
ENV
sed -i 's/^BROADCAST_CONNECTION=.*/BROADCAST_CONNECTION=reverb/; s/^REDIS_CLIENT=.*/REDIS_CLIENT=predis/; s/^REDIS_PORT=.*/REDIS_PORT=6390/; s/^QUEUE_CONNECTION=.*/QUEUE_CONNECTION=redis/; s/^SESSION_DRIVER=.*/SESSION_DRIVER=file/; s/^CACHE_STORE=.*/CACHE_STORE=redis/; s/^MAIL_MAILER=.*/MAIL_MAILER=log/; s#^APP_URL=.*#APP_URL=http://127.0.0.1:8300#' .env
grep -q '^BROADCAST_CONNECTION' .env || echo 'BROADCAST_CONNECTION=reverb' >> .env
"$PAM" artisan filament:install --panels --no-interaction >/dev/null 2>&1 || true
"$PAM" artisan horizon:install >/dev/null 2>&1
"$PAM" artisan vendor:publish --provider='Laravel\Sanctum\SanctumServiceProvider' >/dev/null 2>&1
"$PAM" artisan vendor:publish --tag=reverb-config >/dev/null 2>&1
"$PAM" artisan install:broadcasting --reverb --without-node --no-interaction >/dev/null 2>&1 || true
"$PAM" artisan vendor:publish --tag=laravel-mail >/dev/null 2>&1 || true
cp -R "$READINESS_DIR/ecosystem/overlay/." .
python3 - <<'PY'
import re
p = "bootstrap/app.php"
s = open(p).read()
s = s.replace("health: '/up',", "health: '/up',\n        then: fn () => \\Illuminate\\Support\\Facades\\Route::group([], base_path('routes/eco.php')),")
s = s.replace("->withMiddleware(function (Middleware $middleware): void {",
              "->withMiddleware(function (Middleware $middleware): void {\n        $middleware->web(append: [\\Inertia\\Middleware::class]);\n        $middleware->trustProxies(at: '*');")
open(p, "w").write(s)
web = "routes/web.php"
open(web, "w").write("<?php\n")
console = open("routes/console.php").read()
console += "\n\\Illuminate\\Support\\Facades\\Schedule::call(fn () => \\Illuminate\\Support\\Facades\\Cache::put('readiness-schedule', now()->toIso8601String(), 600))->everyMinute();\n"
open("routes/console.php", "w").write(console)
user = open("app/Models/User.php").read()
if "HasApiTokens" not in user:
    user = user.replace("use HasFactory, Notifiable;", "use HasFactory, Notifiable, \\Laravel\\Sanctum\\HasApiTokens;")
    open("app/Models/User.php", "w").write(user)
PY
touch database/database.sqlite
"$PAM" artisan migrate --force >/dev/null
"$PAM" artisan storage:link >/dev/null 2>&1 || true
"$PAM" artisan tinker --execute='App\Models\User::firstOrCreate(["email"=>"admin@example.test"],["name"=>"Admin","password"=>bcrypt("password")]);' >/dev/null
npm install --silent --no-audit --no-fund vue@3 @inertiajs/vue3@2 @vitejs/plugin-vue laravel-echo pusher-js >/dev/null 2>&1
python3 - <<'PY'
p = "vite.config.js"
s = open(p).read()
if "plugin-vue" not in s:
    s = "import vue from '@vitejs/plugin-vue';\n" + s
    s = s.replace("plugins: [", "plugins: [\n        vue(),", 1)
    s = s.replace("input: ['resources/css/app.css', 'resources/js/app.js']", "input: ['resources/js/app.js']")
open(p, "w").write(s)
PY
npx vite build >/dev/null 2>&1
"$PAM" artisan filament:assets >/dev/null 2>&1 || true
"$PAM" artisan livewire:publish --assets >/dev/null 2>&1 || true
touch .readiness-provisioned
log "ecosystem app ready"
