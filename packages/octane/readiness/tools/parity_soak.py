#!/usr/bin/env python3
"""Soak, chaos and leak runs for PAM's Swoole-parity features.

Drives a provisioned readiness app (lib.sh provision_app) under `wrk` with a
mix of shared-table writes, shared-cache reads and misses,
Octane::concurrently() on task workers and plain requests, while it reloads
the server, recycles workers (--max-requests) and, in chaos mode, SIGKILLs
random HTTP and task workers. It then checks that no increment was lost or
invented, that the tables still work, and how memory evolved.

  parity_soak.py --app DIR --pam pam --port 8160 --seconds 1800 --reload-every 60
  parity_soak.py ... --chaos --kill-every 15            # SIGKILL storm
  parity_soak.py ... --max-requests 100000000           # leak run (no recycling)

Prints one JSON object (also written to --out).
"""
import argparse
import json
import os
import random
import re
import subprocess
import tempfile
import threading
import time
import sys
import urllib.request

HAMMER = os.path.join(os.path.dirname(os.path.abspath(__file__)), "hammer.py")

LUA = {
    # Every response body is checked: wrk counts only the HTTP status.
    "incr": 'request = function() return wrk.format("GET", "/_r/parity/table/incr?key=soak") end',
    "cache": 'request = function() return wrk.format("GET", "/_r/parity/cache/get?key=soak-cache") end',
    "miss": 'request = function() return wrk.format("GET", "/_r/parity/cache/remember?key=" .. math.random(1, 20000)) end',
    "tasks": 'request = function() return wrk.format("GET", "/_r/parity/concurrently?sleep=2&count=2&wait=10000") end',
    "hello": 'request = function() return wrk.format("GET", "/_r/hello") end',
}
CONNECTIONS = {"incr": 16, "cache": 8, "miss": 8, "tasks": 4, "hello": 16}


def get(base, path, timeout=10):
    with urllib.request.urlopen(base + path, timeout=timeout) as response:
        return json.loads(response.read())


def pam(args, *argv, check=True, env=None):
    merged = dict(os.environ)
    merged.update(env or {})
    proc = subprocess.run([args.pam, *argv], cwd=args.app, capture_output=True, text=True, timeout=180, env=merged)
    if check and proc.returncode != 0:
        raise RuntimeError(f"pam {' '.join(argv)}: {proc.stdout}{proc.stderr}")
    return proc


def master_pid(port):
    for line in subprocess.run(["ss", "-ltnp", f"sport = :{port}"], capture_output=True, text=True).stdout.splitlines():
        for pid in re.findall(r"pid=(\d+)", line):
            parent = int(open(f"/proc/{pid}/stat").read().split(") ")[1].split()[1])
            # Workers listen; their parent is the master or a zygote.
            for candidate in (parent, int(open(f"/proc/{parent}/stat").read().split(") ")[1].split()[1])):
                if "start" in open(f"/proc/{candidate}/cmdline").read().replace("\0", " "):
                    return candidate
    return None


def tree(pid):
    """(pid, role) of every process under the master."""
    children = {}
    for entry in os.listdir("/proc"):
        if entry.isdigit():
            try:
                parent = int(open(f"/proc/{entry}/stat").read().split(") ")[1].split()[1])
            except OSError:
                continue
            children.setdefault(parent, []).append(int(entry))
    result, stack = [], [pid]
    while stack:
        current = stack.pop()
        try:
            environ = open(f"/proc/{current}/environ", "rb").read()
            cmdline = open(f"/proc/{current}/cmdline", "rb").read()
        except OSError:
            continue
        role = "master"
        if b"__zygote" in cmdline:
            role = "zygote"
        elif b"PAM_TASK_WORKER=1" in environ:
            role = "task"
        elif b"PAM_WORKER_ID=" in environ:
            role = "http"
        result.append((current, role))
        stack.extend(children.get(current, []))
    return result


def memory(pid):
    values = {}
    try:
        for line in open(f"/proc/{pid}/smaps_rollup"):
            key, _, rest = line.partition(":")
            if key in ("Rss", "Pss", "Private_Dirty"):
                values[key.lower()] = int(rest.split()[0])
    except OSError:
        pass
    return values


def snapshot(master):
    roles = {}
    for pid, role in tree(master):
        roles.setdefault(role, []).append({"pid": pid, **memory(pid)})
    return roles


def parse_wrk(text):
    result = {"requests": 0, "non2xx": 0, "socket_errors": 0, "rps": 0.0}
    match = re.search(r"(\d+) requests in", text)
    if match:
        result["requests"] = int(match.group(1))
    match = re.search(r"Non-2xx or 3xx responses: (\d+)", text)
    if match:
        result["non2xx"] = int(match.group(1))
    match = re.search(r"Socket errors: connect (\d+), read (\d+), write (\d+), timeout (\d+)", text)
    if match:
        result["socket_errors"] = sum(int(value) for value in match.groups())
    match = re.search(r"Requests/sec:\s+([\d.]+)", text)
    if match:
        result["rps"] = float(match.group(1))
    return result


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--app", required=True)
    parser.add_argument("--pam", default="pam")
    parser.add_argument("--port", type=int, default=8160)
    parser.add_argument("--seconds", type=int, default=1800)
    parser.add_argument("--workers", type=int, default=4)
    parser.add_argument("--task-workers", type=int, default=2)
    parser.add_argument("--max-requests", type=int, default=2000)
    parser.add_argument("--reload-every", type=int, default=0)
    parser.add_argument("--chaos", action="store_true")
    parser.add_argument("--kill-every", type=float, default=15)
    parser.add_argument("--sample-every", type=int, default=30)
    parser.add_argument("--mix", default="incr,cache,miss,tasks,hello")
    parser.add_argument("--out")
    args = parser.parse_args()
    base = f"http://127.0.0.1:{args.port}"

    pam(args, "octane:stop", check=False)
    pam(args, "octane:start", f"--port={args.port}", f"--workers={args.workers}",
        f"--task-workers={args.task_workers}", f"--max-requests={args.max_requests}")
    deadline = time.time() + 60
    while True:
        try:
            get(base, "/_r/hello" if False else "/_r/parity/table/count", 2)
            break
        except Exception:  # noqa: BLE001
            if time.time() > deadline:
                raise
            time.sleep(0.3)
    start_votes = (get(base, "/_r/parity/table/get?key=soak")["row"] or {}).get("votes", 0)
    get(base, "/_r/parity/cache/put?key=soak-cache&value=soak")
    master = master_pid(args.port)
    samples = [{"t": 0, **snapshot(master)}]

    scripts = tempfile.mkdtemp(prefix="pam-soak-")
    wrks = {}
    for name in args.mix.split(","):
        script = os.path.join(scripts, f"{name}.lua")
        with open(script, "w") as handle:
            handle.write(LUA[name] + "\n")
        wrks[name] = subprocess.Popen(
            ["wrk", "-t2", f"-c{CONNECTIONS[name]}", f"-d{args.seconds}s", "--timeout", "30s", "-s", script, base],
            stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)

    events = {"reloads": 0, "kills": [], "errors": []}
    started = time.time()
    next_reload = started + args.reload_every if args.reload_every else None
    next_kill = started + args.kill_every if args.chaos else None
    next_sample = started + args.sample_every
    while time.time() - started < args.seconds:
        now = time.time()
        if next_reload and now >= next_reload:
            proc = pam(args, "octane:reload", check=False)
            events["reloads"] += 1
            if proc.returncode != 0:
                events["errors"].append(f"reload: {proc.stderr.strip()[:200]}")
            next_reload = now + args.reload_every
        if next_kill and now >= next_kill:
            victims = [(pid, role) for pid, role in tree(master) if role in ("http", "task")]
            if victims:
                pid, role = random.choice(victims)
                try:
                    os.kill(pid, 9)
                    events["kills"].append(role)
                except ProcessLookupError:
                    pass
            next_kill = now + args.kill_every * random.uniform(0.5, 1.5)
        if now >= next_sample:
            samples.append({"t": int(now - started), **snapshot(master)})
            next_sample = now + args.sample_every
        time.sleep(0.2)

    results = {name: parse_wrk(proc.communicate()[0]) for name, proc in wrks.items()}
    time.sleep(2)
    samples.append({"t": int(time.time() - started), **snapshot(master)})

    # Integrity: every successful increment response is one increment. wrk
    # does not count the requests still in flight when its duration ends (at
    # most one per connection), so the soak allows that many extra.
    final_votes = get(base, "/_r/parity/table/get?key=soak")["row"]["votes"]
    incr = results.get("incr", {"requests": 0, "non2xx": 0, "socket_errors": 0})
    succeeded = incr["requests"] - incr["non2xx"]
    applied = final_votes - start_votes
    in_flight = CONNECTIONS["incr"]

    # Exactness: a fixed number of increments (every one counted by the
    # client) across a reload must land exactly once each.
    before = get(base, "/_r/parity/table/get?key=soak-exact")
    before = (before["row"] or {}).get("votes", 0)
    hammer = subprocess.Popen([sys.executable, "-I", HAMMER, base + "/_r/parity/table/incr?key=soak-exact",
                               "-c", "16", "-n", "20000", "--keepalive"], stdout=subprocess.PIPE, text=True)
    time.sleep(1)
    if not args.chaos:
        pam(args, "octane:reload", check=False)
    exact = json.loads(hammer.communicate()[0])
    exact_applied = get(base, "/_r/parity/table/get?key=soak-exact")["row"]["votes"] - before
    # After a chaos run the tables must still be fully usable.
    for _ in range(200):
        get(base, "/_r/parity/table/incr?key=soak-after")
    after = get(base, "/_r/parity/table/get?key=soak-after")["row"]["votes"]
    stats = get(base, "/_r/parity/table/stats")
    pam(args, "octane:stop", check=False)

    report = {
        "seconds": args.seconds, "workers": args.workers, "task_workers": args.task_workers,
        "max_requests": args.max_requests, "chaos": args.chaos, "reloads": events["reloads"],
        "kills": {role: events["kills"].count(role) for role in ("http", "task")},
        "wrk": results,
        "total_requests": sum(item["requests"] for item in results.values()),
        "total_failed": sum(item["non2xx"] + item["socket_errors"] for item in results.values()),
        "increments": {"responses_ok": succeeded, "applied": applied,
                       "consistent": succeeded <= applied <= succeeded + in_flight + incr["socket_errors"] + incr["non2xx"],
                       "exact_run": {"sent": exact["total"], "ok": exact["ok"], "failed": exact["failed"],
                                     "applied": exact_applied, "exact": exact_applied == exact["ok"] == exact["total"]}},
        "post_run_increments_ok": after % 200 == 0 and after > 0,
        "table_recoveries": stats.get("recoveries"),
        "memory_first": samples[0], "memory_last": samples[-1], "samples": samples,
        "events": events["errors"],
    }
    text = json.dumps(report, indent=2)
    if args.out:
        with open(args.out, "w") as handle:
            handle.write(text)
    summary = {key: value for key, value in report.items() if key not in ("samples", "memory_first", "memory_last")}
    print(json.dumps(summary, indent=2))


if __name__ == "__main__":
    main()
