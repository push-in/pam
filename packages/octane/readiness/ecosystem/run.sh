#!/usr/bin/env bash
# Stage 3: run the ecosystem smoke flow under PAM Octane and under FrankenPHP.
source "$(dirname "$0")/../lib.sh"
APP="${ECO_APP:-$WORK_DIR/eco}"
PORT="${ECO_PORT:-8300}"
OUT="$RESULTS_DIR/ecosystem"
FRANKENPHP="${FRANKENPHP:-$WORK_DIR/bin/frankenphp}"
mkdir -p "$OUT"
bash "$READINESS_DIR/ecosystem/provision.sh"
(cd "$READINESS_DIR/ecosystem/smoke" && [ -d node_modules ] || npm install --silent --no-audit --no-fund)

if [ ! -x "$FRANKENPHP" ]; then
    mkdir -p "$(dirname "$FRANKENPHP")"
    version=$(curl -fsSL https://api.github.com/repos/php/frankenphp/releases/latest | python3 -c 'import json,sys;print(json.load(sys.stdin)["tag_name"])')
    curl -fsSL -o "$FRANKENPHP" "https://github.com/php/frankenphp/releases/download/$version/frankenphp-linux-x86_64"
    chmod +x "$FRANKENPHP"
fi
ln -sf "$FRANKENPHP" "$APP/frankenphp"

docker rm -f pam-readiness-redis >/dev/null 2>&1 || true
docker run -d --rm --name pam-readiness-redis -p 127.0.0.1:6390:6379 redis:7-alpine >/dev/null
trap 'docker rm -f pam-readiness-redis >/dev/null 2>&1 || true' EXIT

run_flow() {
    local server="$1"; shift
    local -a php=("$@")
    local pids=()
    log "ecosystem smoke under $server"
    rm -f "$APP/storage/logs/laravel.log"
    "${php[@]}" artisan horizon > "$OUT/$server-horizon.log" 2>&1 & pids+=($!)
    "${php[@]}" artisan reverb:start --port=8390 > "$OUT/$server-reverb.log" 2>&1 & pids+=($!)
    if [ "$server" = pam ]; then
        (cd "$APP" && "$PAM" octane:start --port="$PORT" --workers=4) > "$OUT/$server-server.log" 2>&1
    else
        (cd "$APP" && "$PAM" artisan octane:start --server=frankenphp --port="$PORT" --workers=4 --max-requests=100000) \
            > "$OUT/$server-server.log" 2>&1 & pids+=($!)
    fi
    wait_http "http://127.0.0.1:$PORT/up" 90
    SMOKE_TMP="$WORK_DIR" node "$READINESS_DIR/ecosystem/smoke/smoke.mjs" "http://127.0.0.1:$PORT" "$APP" "${php[@]}" \
        > "$OUT/$server.json" 2> "$OUT/$server.txt" || true
    cat "$OUT/$server.txt" >&2
    if [ "$server" = pam ]; then (cd "$APP" && "$PAM" octane:stop >/dev/null 2>&1); fi
    for pid in "${pids[@]}"; do kill "$pid" 2>/dev/null; done
    wait "${pids[@]}" 2>/dev/null || true
    pkill -f "$APP/frankenphp" 2>/dev/null || true
    sleep 1
}

cd "$APP"
# READINESS_RESPONSE_CACHE=1 runs the flow with PAM's edge page cache on (and
# cookies allowed, the most permissive setting): session pages must still work.
sed -i '/^PAM_RESPONSE_CACHE/d' .env
if [ "${READINESS_RESPONSE_CACHE:-0}" = 1 ]; then
    printf 'PAM_RESPONSE_CACHE=true\nPAM_RESPONSE_CACHE_ALLOW_COOKIES=true\n' >> .env
fi
SERVERS="${ECO_SERVERS:-pam frankenphp}"
if [[ " $SERVERS " == *" pam "* ]]; then run_flow pam "$PAM"; fi
# Horizon re-executes PHP_BINARY for its workers, which is not usable as a
# CLI under `frankenphp php-cli`; queue/websocket processes use the system
# php-cli next to FrankenPHP, as in typical FrankenPHP deployments.
if [[ " $SERVERS " == *" frankenphp "* ]]; then run_flow frankenphp "${PHP_CLI:-php}"; fi
python3 - "$OUT" <<'PY'
import json, sys
out = sys.argv[1]
rows = {}
for server in ("pam", "frankenphp"):
    try:
        for item in json.load(open(f"{out}/{server}.json")):
            rows.setdefault(item["name"], {})[server] = item
    except Exception as error:
        print(f"{server}: no results ({error})", file=sys.stderr)
with open(f"{out}/summary.md", "w") as md:
    md.write("| Flow step | PAM Octane | FrankenPHP |\n|---|---|---|\n")
    for name, servers in rows.items():
        cell = lambda s: servers.get(s, {}).get("status", "-")
        md.write(f"| {name} | {cell('pam')} | {cell('frankenphp')} |\n")
print(open(f"{out}/summary.md").read())
PY
