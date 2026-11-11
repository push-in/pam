#!/usr/bin/env bash
# Long stress run that watches the master for an unexplained exit.
#
#   master_hunt.sh WORK_DIR PAM_ROOT SECONDS
#
# Two servers with 4 HTTP and 2 task workers each, both driven by
# parity_stress.py in a loop (parallel table writers, cache misses, clients
# that hang up, slowloris, memory_limit fatals) plus a reload every 60 s:
#   A  WORK_DIR/l13 on :8180, managed by pamd (pamd logs how its masters end)
#   B  WORK_DIR/l12 on :8190, the master in the foreground under gdb, which
#      prints a backtrace if it dies of a signal it can catch
# Every master change of A and the exit of B are logged in WORK_DIR/hunt/.
set -uo pipefail
WORK="$1"; ROOT="$2"; SECONDS_TOTAL="$3"
TOOLS="$(cd "$(dirname "$0")" && pwd)"
OUT="$WORK/hunt"; mkdir -p "$OUT"
PAM="$ROOT/bin/pam-run"
export PAM_HOME="$ROOT/share/pam" PAM_EXTENSION_DIR="$ROOT/lib/php/extensions" \
    PHPRC="$ROOT/etc/php.ini" PHP_INI_SCAN_DIR="$ROOT/etc/conf.d" PATH="$ROOT/bin:$PATH"
ulimit -c unlimited

# A: through pamd, in its own manager state.
export PAM_MANAGER_STATE_DIR="$OUT/state" PAM_MANAGER_RUNTIME_DIR="$OUT/state/runtime"
mkdir -p "$PAM_MANAGER_RUNTIME_DIR"; chmod 700 "$PAM_MANAGER_RUNTIME_DIR"
(cd "$WORK/l13" && "$PAM" octane:start --port=8180 --workers=4 --task-workers=2 --max-requests=100000) >> "$OUT/a.log" 2>&1
state_a=$(ls "$PAM_MANAGER_RUNTIME_DIR"/*.master.json)

# B: foreground master under gdb.
(cd "$WORK/l12" && gdb -q -batch \
    -ex "handle SIGPIPE SIGHUP SIGCHLD SIGUSR1 SIGUSR2 SIGTERM SIGINT SIGALRM SIGWINCH SIGIO SIGURG SIGPROF nostop noprint pass" \
    -ex "set follow-fork-mode parent" -ex "set detach-on-fork on" -ex "set print thread-events off" \
    -ex run -ex "info program" -ex "thread apply all bt" \
    --args "$ROOT/bin/pam" start artisan --workers 4 --max-requests 100000 --task-workers 2 -- pam:octane --port=8190) \
    > "$OUT/b-gdb.log" 2>&1 &
gdb_pid=$!
until curl -fs -o /dev/null http://127.0.0.1:8190/_r/hello; do sleep 0.5; done
until curl -fs -o /dev/null http://127.0.0.1:8180/_r/hello; do sleep 0.5; done
# The subshell's child is gdb; gdb's child is the master.
gdb_process=$(pgrep -P "$gdb_pid" -x gdb | head -1)
master_b=$(pgrep -P "$gdb_process" -x pam | head -1)
echo "$(date +%T) start A=$(python3 -c "import json;print(json.load(open('$state_a'))['pid'])") B=$master_b gdb=$gdb_pid" >> "$OUT/events.log"

deadline=$((SECONDS + SECONDS_TOTAL))
stress() {
    local port=$1 n=0
    while [ $SECONDS -lt $deadline ]; do
        n=$((n + 1))
        python3 "$TOOLS/parity_stress.py" --port "$port" --rounds 5 --out "$OUT/stress-$port-$n.json" > /dev/null 2>&1
        echo "$(date +%T) stress :$port run $n rc=$? $(python3 -c "import json;print(json.load(open('$OUT/stress-$port-$n.json'))['ok'])" 2>/dev/null)" >> "$OUT/events.log"
    done
}
stress 8180 & s1=$!
stress 8190 & s2=$!

last_a=""
next_reload=$((SECONDS + 60))
while [ $SECONDS -lt $deadline ]; do
    a=$(python3 -c "import json;print(json.load(open('$state_a'))['pid'])" 2>/dev/null || echo none)
    [ -n "$last_a" ] && [ "$a" != "$last_a" ] && echo "$(date +%T.%N) A master changed $last_a -> $a" >> "$OUT/events.log"
    last_a=$a
    if ! kill -0 "$master_b" 2>/dev/null; then
        echo "$(date +%T.%N) B master $master_b gone" >> "$OUT/events.log"; break
    fi
    if [ $SECONDS -ge $next_reload ]; then
        (cd "$WORK/l13" && "$PAM" octane:reload >/dev/null 2>&1)
        kill -HUP "$master_b"
        next_reload=$((SECONDS + 60))
    fi
    sleep 0.2
done
wait $s1 $s2
(cd "$WORK/l13" && "$PAM" octane:stop >/dev/null 2>&1); "$PAM" daemon stop >/dev/null 2>&1
kill -TERM "$master_b" 2>/dev/null; wait "$gdb_pid"
echo "$(date +%T) end" >> "$OUT/events.log"
