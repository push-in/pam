#!/usr/bin/env python3
"""PAM Octane feature matrix.

Drives a provisioned Laravel app (see ../lib.sh provision_app) through
`pam octane:start|status|reload|stop` and checks every Laravel Octane feature.
Each check reports PASS / FAIL / N/A (feature is Swoole-only in Octane itself).

  matrix.py --app /path/to/app --pam pam --port 8100 --out results.json
"""
import argparse
import hashlib
import http.client
import json
import os
import re
import shutil
import socket
import ssl
import subprocess
import sys
import tempfile
import threading
import time
import urllib.request

HERE = os.path.dirname(os.path.abspath(__file__))
HAMMER = os.path.join(HERE, "..", "tools", "hammer.py")
RESULTS = []


def record(feature, status, detail="", category="pam"):
    RESULTS.append({"feature": feature, "status": status, "detail": detail, "category": category})
    print(f"[{status:4}] {feature}: {detail}", flush=True)


class App:
    def __init__(self, args):
        self.dir = args.app
        self.pam = args.pam
        self.port = args.port
        self.base = f"http://127.0.0.1:{self.port}"
        self.env_file = os.path.join(self.dir, "readiness.env")

    def pam_cmd(self, *argv, check=False, timeout=120, env=None):
        merged = dict(os.environ)
        merged.update(env or {})
        proc = subprocess.run([self.pam, *argv], cwd=self.dir, capture_output=True, text=True,
                              timeout=timeout, env=merged)
        if check and proc.returncode != 0:
            raise RuntimeError(f"pam {' '.join(argv)} failed: {proc.stdout}{proc.stderr}")
        return proc

    def start(self, *extra, server_env=None, ini=None):
        self.stop(quiet=True)
        argv = ["octane:start", f"--port={self.port}", "--env-file", self.env_file, *extra]
        # READINESS_RESPONSE_CACHE=1 runs every check with the edge page cache on.
        values = {"PAM_RESPONSE_CACHE": os.environ.get("READINESS_RESPONSE_CACHE", "0")}
        values.update(server_env or {})
        self.write_env(values, ini or {})
        self.pam_cmd(*argv, check=True)
        self.wait()

    @property
    def upload_dir(self):
        return os.path.join(self.dir, "readiness-upload-tmp")

    def write_env(self, values, ini_overrides=None):
        ini = os.path.join(self.dir, "readiness-php.ini")
        os.makedirs(self.upload_dir, exist_ok=True)
        # memory_limit stays at PHP's default (128M): uploads are streamed to
        # disk, so their size is bounded by upload_max_filesize/post_max_size.
        settings = {"upload_max_filesize": "2G", "post_max_size": "2G", "upload_tmp_dir": self.upload_dir}
        settings.update(ini_overrides or {})
        with open(ini, "w") as handle:
            handle.write("".join(f"{key}={value}\n" for key, value in settings.items()))
        # PAM_MAX_BODY_BYTES bounds in-memory bodies (the 5 MB JSON check);
        # multipart uploads do not depend on it.
        lines = [f"PHPRC={ini}", "PAM_MAX_BODY_BYTES=16777216"]
        lines += [f"{key}={value}" for key, value in values.items()]
        fd = os.open(self.env_file, os.O_WRONLY | os.O_CREAT | os.O_TRUNC, 0o600)
        with os.fdopen(fd, "w") as handle:
            handle.write("\n".join(lines) + "\n")
        os.chmod(self.env_file, 0o600)

    def stop(self, quiet=False):
        proc = self.pam_cmd("octane:stop")
        if not quiet and proc.returncode != 0:
            raise RuntimeError(proc.stderr)
        deadline = time.time() + 15
        while time.time() < deadline and self.listening():
            time.sleep(0.2)

    def listening(self, scheme="http"):
        try:
            with socket.create_connection(("127.0.0.1", self.port), timeout=0.5):
                return True
        except OSError:
            return False

    def wait(self, path="/_r/hello", timeout=60, tls=False):
        deadline = time.time() + timeout
        while time.time() < deadline:
            try:
                status, _, _ = self.get(path, tls=tls, timeout=2)
                if status == 200:
                    return
            except Exception:
                pass
            time.sleep(0.3)
        raise RuntimeError(f"server did not answer {path}")

    def get(self, path, headers=None, method="GET", body=None, tls=False, timeout=30):
        if tls:
            conn = http.client.HTTPSConnection("127.0.0.1", self.port, timeout=timeout,
                                               context=ssl._create_unverified_context())
        else:
            conn = http.client.HTTPConnection("127.0.0.1", self.port, timeout=timeout)
        conn.request(method, path, body=body, headers=headers or {})
        response = conn.getresponse()
        data = response.read()
        conn.close()
        return response.status, dict((k.lower(), v) for k, v in response.getheaders()), data

    def json(self, path, **kwargs):
        status, headers, data = self.get(path, **kwargs)
        return status, json.loads(data) if data[:1] in (b"{", b"[") else data

    def hammer(self, path, *extra):
        out = subprocess.run([sys.executable, "-I", HAMMER, self.base + path, *extra],
                             capture_output=True, text=True, check=True)
        return json.loads(out.stdout)

    def worker_rss_kb(self, pid):
        with open(f"/proc/{pid}/status") as handle:
            for line in handle:
                if line.startswith("VmRSS:"):
                    return int(line.split()[1])
        return 0


def check(name, category="pam"):
    def wrap(fn):
        def run(app):
            try:
                fn(app)
            except Exception as error:  # noqa: BLE001
                record(name, "FAIL", f"{type(error).__name__}: {error}", category)
        run.__name__ = fn.__name__
        return run
    return wrap


# --- checks ---------------------------------------------------------------

@check("Basic responses (JSON, Blade, DB, redirect, 204, HEAD, cookies, repeated headers)")
def basics(app):
    problems = []
    if app.get("/_r/json")[0] != 200:
        problems.append("json")
    status, _, body = app.get("/_r/view")
    if status != 200 or b"item-20" not in body:
        problems.append("view")
    if app.json("/_r/db")[0] != 200:
        problems.append("db")
    status, headers, _ = app.get("/_r/redirect")
    if status != 302 or not headers.get("location", "").endswith("/_r/hello"):
        problems.append("redirect")
    if app.get("/_r/empty")[0] != 204:
        problems.append("204")
    status, _, body = app.get("/_r/hello", method="HEAD")
    if status != 200 or body:
        problems.append("HEAD")
    if app.get("/_r/abort")[0] != 418:
        problems.append("abort")
    for _ in range(3):
        status, _, body = app.get("/_r/send")
        if status != 200 or not body.startswith(b"sent") or app.get("/_r/hello")[0] != 200:
            problems.append(f"response()->send() HTTP {status}")
            break
    conn = http.client.HTTPConnection("127.0.0.1", app.port, timeout=10)
    conn.request("GET", "/_r/cookie")
    response = conn.getresponse()
    response.read()
    cookies = response.msg.get_all("set-cookie") or []
    multi = response.msg.get_all("x-multi") or []
    if len(cookies) < 2 or sorted(multi) != ["one", "two"] and multi != ["one, two"]:
        problems.append(f"cookies={len(cookies)} x-multi={multi}")
    record("Basic responses", "PASS" if not problems else "FAIL", ", ".join(problems) or "all correct")


@check("Request isolation / state reset")
def isolation(app):
    app.get("/_r/state/write?v=leak")
    leaks = []
    for _ in range(12):  # cover every worker
        status, state = app.json("/_r/state/read")
        for key, clean in [("instance", None), ("config", None), ("locale", "en"), ("auth", False),
                           ("session", None), ("attribute", None), ("request_query", None)]:
            if state.get(key) != clean:
                leaks.append(f"{key}={state.get(key)!r}@{state.get('pid')}")
    status, state = app.json("/_r/state/singleton", headers={"X-Probe": "one"})
    status, again = app.json("/_r/state/singleton", headers={"X-Probe": "two"})
    if state.get("header") != "one" or again.get("header") != "two":
        leaks.append("request instance reused")
    record("Request isolation (container instances, config, locale, auth, session, request)",
           "PASS" if not leaks else "FAIL", "no state leaked across 12 requests" if not leaks else "; ".join(sorted(set(leaks))[:6]))


@check("Memory stability over 50k requests")
def memory(app):
    app.start("--workers=1")
    app.hammer("/_r/json", "-c", "8", "-n", "5000", "--keepalive")
    _, warm = app.json("/_r/pid")
    rss_warm = app.worker_rss_kb(warm["pid"])
    result = app.hammer("/_r/json", "-c", "8", "-n", "50000", "--keepalive")
    _, after = app.json("/_r/pid")
    rss_after = app.worker_rss_kb(after["pid"])
    same = warm["pid"] == after["pid"]
    php_growth = after["memory"] - warm["memory"]
    rss_growth = rss_after - rss_warm
    ok = same and result["failed"] == 0 and php_growth < 1_048_576 and rss_growth < 16_384
    record("Memory stability over 50k requests (1 worker)", "PASS" if ok else "FAIL",
           f"{result['ok']} ok/{result['failed']} failed, PHP heap {warm['memory']//1024}->{after['memory']//1024} KiB, "
           f"RSS {rss_warm//1024}->{rss_after//1024} MiB, same worker={same}, {result['rps']} rps")


@check("--max-requests worker recycling")
def max_requests(app):
    app.start("--workers=4", "--max-requests=500")
    for keepalive in (False, True):
        pids = {app.json("/_r/pid")[1]["pid"] for _ in range(40)}
        before = len(pids)
        extra = ["--keepalive"] if keepalive else []
        result = app.hammer("/_r/hello", "-c", "16", "-s", "12", *extra)
        pids |= {app.json("/_r/pid")[1]["pid"] for _ in range(40)}
        ok = len(pids) > before and result["failed"] == 0
        record(f"--max-requests recycling under load (4 workers, limit 500, "
               f"{'keep-alive' if keepalive else 'new connection per request'})", "PASS" if ok else "FAIL",
               f"{len(pids)} distinct worker pids, {result['ok']} ok/{result['failed']} failed {result['breakdown']}")


@check("octane:reload under load")
def reload(app):
    app.start("--workers=4")
    results = {}

    def load():
        results["r"] = app.hammer("/_r/hello", "-c", "16", "-s", "14", "--keepalive")

    thread = threading.Thread(target=load)
    thread.start()
    pids_before = {app.json("/_r/pid")[1]["pid"] for _ in range(20)}
    for _ in range(4):
        time.sleep(2.5)
        app.pam_cmd("octane:reload", check=True)
    thread.join()
    pids_after = {app.json("/_r/pid")[1]["pid"] for _ in range(20)}
    r = results["r"]
    ok = r["failed"] == 0 and pids_before.isdisjoint(pids_after)
    record("octane:reload (4 reloads, keep-alive) with zero dropped requests", "PASS" if ok else "FAIL",
           f"{r['ok']} ok/{r['failed']} failed {r['breakdown']}, workers replaced={pids_before.isdisjoint(pids_after)}")


@check("octane:status / octane:stop / restart")
def lifecycle(app):
    app.start("--workers=2")
    status = app.pam_cmd("octane:status")
    online = status.returncode == 0 and "online" in (status.stdout + status.stderr).lower()
    app.stop()
    stopped = not app.listening()
    app.start("--workers=2")  # stop -> start must work for the same project
    restarted = app.get("/_r/hello")[0] == 200
    ok = online and stopped and restarted
    record("octane:status / octane:stop / start again", "PASS" if ok else "FAIL",
           f"status online={online}, stop closed port={stopped}, restart={restarted}")


@check("--watch file reload")
def watch(app):
    app.stop(quiet=True)
    routes = os.path.join(app.dir, "routes", "readiness.php")
    original = open(routes).read()
    env = dict(os.environ, PHPRC=os.path.join(app.dir, "readiness-php.ini"))
    proc = subprocess.Popen([app.pam, "octane:start", "--watch", f"--port={app.port}"], cwd=app.dir,
                            stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, env=env,
                            start_new_session=True)
    try:
        app.wait("/_r/watch")
        before = app.get("/_r/watch")[2]
        with open(routes, "w") as handle:
            handle.write(original.replace("'watch-v1'", "'watch-v2'"))
        deadline = time.time() + 20
        after = before
        while time.time() < deadline and after != b"watch-v2":
            time.sleep(0.5)
            try:
                after = app.get("/_r/watch", timeout=3)[2]
            except Exception:
                pass
        ok = before == b"watch-v1" and after == b"watch-v2"
        record("--watch reloads on file change", "PASS" if ok else "FAIL", f"{before!r} -> {after!r}")
    finally:
        with open(routes, "w") as handle:
            handle.write(original)
        os.killpg(proc.pid, 15)
        proc.wait(timeout=20)
        deadline = time.time() + 10
        while time.time() < deadline and app.listening():
            time.sleep(0.2)


@check("Octane::concurrently")
def concurrently(app):
    status, body = app.json("/_r/concurrently")
    results = body.get("results") or [None, None, None]
    ok = status == 200 and results[0] == 2 and isinstance(results[1], int) and results[1] >= 100 and results[2] == "PAM"
    record("Octane::concurrently()", "PASS" if ok else "FAIL",
           f"results={body.get('results')} via {body.get('dispatcher', '').split(chr(92))[-1]} "
           "(sequential outside Swoole, same as RoadRunner/FrankenPHP)")


def parallel(fn, count, threads=32):
    results, lock = [], threading.Lock()
    queue = list(range(count))

    def run():
        while True:
            with lock:
                if not queue:
                    return
                index = queue.pop()
            value = fn(index)
            with lock:
                results.append(value)

    workers = [threading.Thread(target=run) for _ in range(threads)]
    for worker in workers:
        worker.start()
    for worker in workers:
        worker.join()
    return results


@check("Swoole-parity features (tables, cache, tasks, ticks)", "swoole-parity")
def swoole_only(app):
    """Octane::table(), the shared `octane` cache, task workers, ticks and
    intervals: Swoole-only in Octane itself, provided by PAM >= 2.2.15."""
    # Without task workers the `octane` store keeps its per-worker behavior.
    status, body = app.json("/_r/octane-cache")
    record("Octane cache store (`octane` driver)", "PASS" if status == 200 and body.get("ok") else "FAIL",
           f"{body.get('driver', body)} without --task-workers (per worker, as before)")
    try:
        app.start("--workers=4", "--task-workers=2")
        status, body = app.json("/_r/parity/table/count")
    except Exception as error:  # noqa: BLE001 - older runtimes reject --task-workers
        status, body = 0, str(error)
    if status != 200 or "PamTable" not in str(body):
        for feature in ("Octane::table()", "Octane cache store (`octane` driver, shared)", "Octane::concurrently() on task workers",
                        "Task dispatch (--task-workers)", "Octane::tick()", "Octane cache intervals"):
            record(feature, "N/A", f"needs PAM >= 2.2.15 ({status})", "swoole-only")
        app.start("--workers=2")
        return

    # Tables: one row written by one worker is read by every worker; 400
    # concurrent increments spread over all workers lose nothing.
    key = f"t{os.getpid()}{int(time.time())}"
    app.json(f"/_r/parity/table/set?key={key}&name=pam&votes=0")
    seen = parallel(lambda _: app.json(f"/_r/parity/table/get?key={key}")[1], 60, 12)
    pids = {item["pid"] for item in seen}
    rows_ok = all(item["row"] == {"name": "pam", "votes": 0} for item in seen)
    incr = parallel(lambda _: app.json(f"/_r/parity/table/incr?key={key}")[1], 400, 32)
    final = app.json(f"/_r/parity/table/get?key={key}")[1]["row"]["votes"]
    distinct = len({item["votes"] for item in incr})
    ok = rows_ok and len(pids) >= 2 and final == 400 and distinct == 400
    record("Octane::table()", "PASS" if ok else "FAIL",
           f"same row in {len(pids)} workers; 400 concurrent incr -> {final} ({distinct} distinct values)", "swoole-parity")

    # Cache: a value put by one worker is read by all of them.
    app.json(f"/_r/parity/cache/put?key={key}&value=shared")
    seen = parallel(lambda _: app.json(f"/_r/parity/cache/get?key={key}")[1], 60, 12)
    pids = {item["pid"] for item in seen}
    values = {item["value"] for item in seen}
    counter = [app.json(f"/_r/parity/cache/incr?key=c{key}")[1]["value"] for _ in range(20)]
    ok = values == {"shared"} and len(pids) >= 2 and counter == list(range(1, 21))
    record("Octane cache store (`octane` driver, shared)", "PASS" if ok else "FAIL",
           f"value seen by {len(pids)} workers; increments {counter[0]}..{counter[-1]}", "swoole-parity")

    # concurrently(): 4 tasks of 300 ms on 2 task workers finish in ~600 ms
    # (sequential: 1200 ms) and run outside the HTTP worker.
    status, body = app.json("/_r/parity/concurrently?sleep=300&count=2")
    status4, body4 = app.json("/_r/parity/concurrently?sleep=300&count=4")
    _, error = app.json("/_r/parity/concurrently/error")
    ok = (status == 200 and body["ms"] < 550 and len(set(body["pids"])) == 2 and body["worker"] not in body["pids"]
          and status4 == 200 and body4["ms"] < 1000 and "PamTaskDispatcher" in body["dispatcher"]
          and error.get("class", "").endswith("TaskException") and "on purpose" in error.get("message", ""))
    record("Octane::concurrently() on task workers", "PASS" if ok else "FAIL",
           f"2x300 ms in {body.get('ms')} ms on {len(set(body.get('pids', [])))} task workers, 4x300 ms in {body4.get('ms')} ms; "
           f"exceptions -> {error.get('class', '').split(chr(92))[-1]}", "swoole-parity")

    before = app.json("/_r/parity/dispatched")[1]["value"] or 0
    for _ in range(5):
        app.json("/_r/parity/dispatch")
    deadline = time.time() + 10
    after = before
    while time.time() < deadline and after < before + 5:
        time.sleep(0.2)
        after = app.json("/_r/parity/dispatched")[1]["value"] or 0
    record("Task dispatch (--task-workers)", "PASS" if after == before + 5 else "FAIL",
           f"5 dispatched tasks ran ({before} -> {after})", "swoole-parity")

    first = app.json("/_r/parity/ticks")[1]["ticks"] or 0
    time.sleep(3.5)
    second = app.json("/_r/parity/ticks")[1]["ticks"] or 0
    record("Octane::tick()", "PASS" if 2 <= second - first <= 5 else "FAIL",
           f"{second - first} ticks in 3.5 s (one per second, task workers)", "swoole-parity")

    first = app.json("/_r/parity/interval")[1]["value"]
    time.sleep(2.2)
    second = app.json("/_r/parity/interval")[1]["value"]
    record("Octane cache intervals", "PASS" if first and second and first != second else "FAIL",
           f"{first} -> {second} after 2.2 s", "swoole-parity")

    # Rows survive a reload (new worker generation) and a killed worker.
    app.pam_cmd("octane:reload", check=True)
    time.sleep(3)
    app.wait()
    after_reload = app.json(f"/_r/parity/table/get?key={key}")[1]["row"]
    cache_after = app.json(f"/_r/parity/cache/get?key={key}")[1]["value"]
    victim = app.json("/_r/pid")[1].get("pid") if app.get("/_r/pid")[0] == 200 else None
    if victim:
        os.kill(int(victim), 9)
        time.sleep(1.5)
        app.wait()
    after_kill = app.json(f"/_r/parity/table/get?key={key}")[1]["row"]
    ok = after_reload == {"name": "pam", "votes": 400} and cache_after == "shared" and after_kill == after_reload
    record("Tables and cache survive reload and a killed worker", "PASS" if ok else "FAIL",
           f"after reload {after_reload}, cache {cache_after!r}, after SIGKILL of {victim}: {after_kill}", "swoole-parity")
    app.start("--workers=2")


def stream_upload(app, path, size, field="file", filename="big.bin", extra_parts=b"", timeout=600):
    """POSTs a `size`-byte file as multipart without holding it in memory."""
    boundary = "readiness" + os.urandom(8).hex()
    head = (extra_parts.replace(b"BOUNDARY", boundary.encode())
            + f"--{boundary}\r\nContent-Disposition: form-data; name=\"{field}\"; filename=\"{filename}\"\r\n"
              f"Content-Type: application/octet-stream\r\n\r\n".encode())
    tail = f"\r\n--{boundary}--\r\n".encode()
    block = os.urandom(1024 * 1024)
    digest = hashlib.sha256()

    def body():
        yield head
        remaining = size
        while remaining > 0:
            chunk = block[:min(len(block), remaining)]
            digest.update(chunk)
            remaining -= len(chunk)
            yield chunk
        yield tail

    conn = http.client.HTTPConnection("127.0.0.1", app.port, timeout=timeout)
    conn.request("POST", path, body=body(), headers={
        "Content-Type": f"multipart/form-data; boundary={boundary}",
        "Content-Length": str(len(head) + size + len(tail)),
        "Accept": "application/json",
    })
    response = conn.getresponse()
    data = response.read()
    conn.close()
    return response.status, (json.loads(data) if data[:1] == b"{" else {}), digest.hexdigest()


def worker_peak_rss_kb(pid):
    try:
        with open(f"/proc/{pid}/status") as handle:
            for line in handle:
                if line.startswith("VmHWM:"):
                    return int(line.split()[1])
    except OSError:
        pass
    return 0


@check("Large uploads streamed to disk (100 MB, 1 GB)")
def uploads(app):
    # Default memory_limit (128M); only upload_max_filesize/post_max_size raised.
    app.start("--workers=2")
    for label, size in (("100 MB", 100 * 1024 * 1024), ("1 GB", 1024 * 1024 * 1024)):
        for move in (False, True):
            started = time.time()
            status, result, digest = stream_upload(app, "/_r/upload" + ("?move=1" if move else ""), size)
            elapsed = time.time() - started
            leftovers = os.listdir(app.upload_dir)
            peak_kb = worker_peak_rss_kb(result.get("pid", 0))
            ok = (status == 200 and result.get("sha256") == digest and result.get("size") == size
                  and result.get("ok") is True and result.get("moved") is move and not leftovers
                  and result.get("memory_limit") == "128M" and peak_kb < 400 * 1024)
            record(f"{label} multipart upload{' + move()' if move else ''}, memory_limit=128M",
                   "PASS" if ok else "FAIL",
                   f"HTTP {status}, sha256 match={result.get('sha256') == digest}, {elapsed:.2f}s, "
                   f"isValid={result.get('ok')}, PHP peak {result.get('peak_memory', 0)//1048576} MiB, "
                   f"worker peak RSS {peak_kb//1024} MiB, temp files left={len(leftovers)}")

    big_json = json.dumps({"blob": "x" * (5 * 1024 * 1024)}).encode()
    status, _, data = app.get("/_r/echo", method="POST", body=big_json, headers={"Content-Type": "application/json"})
    echo = json.loads(data) if data[:1] == b"{" else {}
    ok = status == 200 and echo.get("sha256") == hashlib.sha256(big_json).hexdigest()
    record("5 MB raw JSON body", "PASS" if ok else "FAIL", f"HTTP {status}")

    # PHP's limits are enforced while streaming.
    app.start("--workers=2", ini={"upload_max_filesize": "1M", "post_max_size": "8M", "max_file_uploads": "2"})
    field = b"--BOUNDARY\r\nContent-Disposition: form-data; name=\"note\"\r\n\r\nhello\r\n"
    status, result, _ = stream_upload(app, "/_r/upload-errors", 2 * 1024 * 1024, extra_parts=field)
    too_big_file = status == 200 and result.get("files", {}).get("file") == 1 and result.get("fields", {}).get("note") == "hello"
    status_post, _, _ = stream_upload(app, "/_r/upload-errors", 9 * 1024 * 1024)
    leftovers = os.listdir(app.upload_dir)
    ok = too_big_file and status_post == 413 and not leftovers
    record("upload_max_filesize / post_max_size enforced while streaming", "PASS" if ok else "FAIL",
           f"2 MB file over 1M limit -> error={result.get('files', {}).get('file')} (want 1, fields kept), "
           f"9 MB body over 8M post_max_size -> HTTP {status_post} (want 413), temp files left={len(leftovers)}")
    app.start("--workers=2")


def progressive(app, path):
    conn = http.client.HTTPConnection("127.0.0.1", app.port, timeout=15)
    started = time.time()
    conn.request("GET", path)
    response = conn.getresponse()
    first = None
    chunks = b""
    while True:
        piece = response.read1(65536) if hasattr(response, "read1") else response.read(65536)
        if not piece:
            break
        if first is None:
            first = time.time() - started
        chunks += piece
    total = time.time() - started
    conn.close()
    return response.status, response.getheader("transfer-encoding"), first or total, total, chunks


@check("Streamed responses / SSE / downloads")
def streaming(app):
    for path, marker in [("/_r/stream", b"chunk-5"), ("/_r/stream-generator", b"gen-5"), ("/_r/sse", b'"tick":5')]:
        status, encoding, first, total, body = progressive(app, path)
        ok = status == 200 and marker in body and first < total * 0.5 and total >= 0.8
        record(f"Progressive {path}", "PASS" if ok else "FAIL",
               f"HTTP {status}, first byte {first*1000:.0f} ms, complete {total*1000:.0f} ms, {encoding or 'content-length'}")
    status, headers, body = app.get("/_r/download", timeout=60)
    expected = b"".join(bytes([65 + (i % 26)]) * (1024 * 1024) for i in range(50))
    ok = status == 200 and body == expected and "readiness.bin" in headers.get("content-disposition", "")
    record("50 MB BinaryFileResponse download", "PASS" if ok else "FAIL", f"HTTP {status}, {len(body)} bytes")
    status, headers, body = app.get("/_r/stream-download", timeout=60)
    ok = status == 200 and len(body) == 64 * 256 * 1024 and set(body) == {ord("x")}
    record("Chunked streamDownload (16 MB)", "PASS" if ok else "FAIL",
           f"HTTP {status}, {len(body)} bytes, {headers.get('transfer-encoding', 'content-length')}")
    still = app.get("/_r/hello")[0] == 200
    record("Worker healthy after streaming", "PASS" if still else "FAIL", "")
    statuses = []

    def quick():
        try:
            statuses.append(app.get("/_r/hello", timeout=10)[0])
        except Exception as error:
            statuses.append(type(error).__name__)

    def slow_download():
        conn = http.client.HTTPConnection("127.0.0.1", app.port, timeout=30)
        conn.request("GET", "/_r/download")
        response = conn.getresponse()
        for _ in range(60):  # hold the worker on client backpressure
            if not response.read(65536):
                break
            time.sleep(0.02)
        conn.close()

    streams = [threading.Thread(target=slow_download) for _ in range(4)]
    for thread in streams:
        thread.start()
    time.sleep(0.2)
    quick_threads = [threading.Thread(target=quick) for _ in range(20)]
    for thread in quick_threads:
        thread.start()
    for thread in streams + quick_threads:
        thread.join()
    ok = statuses.count(200) == 20
    record("Requests arriving while every worker streams are queued (no 503)", "PASS" if ok else "FAIL",
           f"{statuses.count(200)}/20 ok, others={[s for s in statuses if s != 200][:5]}")


@check("Static files from public/")
def static_files(app):
    build = os.path.join(app.dir, "public", "build")
    os.makedirs(build, exist_ok=True)
    asset = os.urandom(300_000).hex().encode()
    with open(os.path.join(build, "readiness.js"), "wb") as handle:
        handle.write(asset)
    storage = os.path.join(app.dir, "storage", "app", "public")
    os.makedirs(storage, exist_ok=True)
    with open(os.path.join(storage, "readiness.txt"), "w") as handle:
        handle.write("stored")
    link = os.path.join(app.dir, "public", "storage")
    if not os.path.islink(link):
        os.symlink(storage, link)
    problems = []
    status, headers, body = app.get("/build/readiness.js")
    if status != 200 or body != asset or "javascript" not in headers.get("content-type", ""):
        problems.append(f"asset {status} {headers.get('content-type')}")
    etag = headers.get("etag")
    if etag and app.get("/build/readiness.js", headers={"If-None-Match": etag})[0] != 304:
        problems.append("no 304 for If-None-Match")
    status, headers, body = app.get("/build/readiness.js", headers={"Range": "bytes=0-99"})
    if status != 206 or body != asset[:100]:
        problems.append(f"range {status} {len(body)}")
    if app.get("/robots.txt")[0] != 200:
        problems.append("robots.txt")
    status, _, body = app.get("/storage/readiness.txt")
    if status != 200 or body != b"stored":
        problems.append(f"storage link {status}")
    status, _, body = app.get("/index.php")
    if b"<?php" in body:
        problems.append("PHP source exposed")
    if app.get("/../.env")[0] == 200 and b"APP_KEY" in app.get("/../.env")[2]:
        problems.append(".env exposed")
    record("Static files (Vite build, robots.txt, storage:link, 304, Range)", "PASS" if not problems else "FAIL",
           ", ".join(problems) or "served with MIME, ETag/304 and Range; no PHP source or files outside public/")


@check("HTTP/1.1 keep-alive")
def keepalive(app):
    conn = http.client.HTTPConnection("127.0.0.1", app.port, timeout=10)
    sockets = set()
    for _ in range(50):
        conn.request("GET", "/_r/hello")
        response = conn.getresponse()
        response.read()
        sockets.add(conn.sock.getsockname() if conn.sock else None)
    conn.close()
    ok = len(sockets) == 1 and None not in sockets
    record("HTTP/1.1 keep-alive (50 requests, 1 connection)", "PASS" if ok else "FAIL", f"{len(sockets)} connection(s)")


@check("Trusted proxies")
def proxies(app):
    status, body = app.json("/_r/proxy", headers={
        "X-Forwarded-For": "203.0.113.9", "X-Forwarded-Proto": "https",
        "X-Forwarded-Host": "shop.example", "X-Forwarded-Port": "443"})
    ok = body.get("secure") is True and body.get("ip") == "203.0.113.9" and body.get("url") == "https://shop.example/x"
    record("Trusted proxies (X-Forwarded-*)", "PASS" if ok else "FAIL", json.dumps(body))


@check("HTTPS")
def https(app):
    workdir = tempfile.mkdtemp(prefix="pam-tls-")
    cert, key = os.path.join(workdir, "cert.pem"), os.path.join(workdir, "key.pem")
    subprocess.run(["openssl", "req", "-x509", "-newkey", "rsa:2048", "-nodes", "-keyout", key, "-out", cert,
                    "-days", "1", "-subj", "/CN=127.0.0.1"], check=True, capture_output=True)
    app.stop(quiet=True)
    app.write_env({"PAM_TLS_CERT": cert, "PAM_TLS_KEY": key})
    app.pam_cmd("octane:start", f"--port={app.port}", "--env-file", app.env_file, "--workers=2", check=True)
    try:
        app.wait(tls=True)
        status, _, body = app.get("/_r/proxy", tls=True)
        secure = json.loads(body).get("secure")
        h2 = subprocess.run(["curl", "-sk", "--http2", "-o", "/dev/null", "-w", "%{http_version}",
                             f"https://127.0.0.1:{app.port}/_r/hello"], capture_output=True, text=True).stdout
        ok = status == 200 and secure is True
        record("HTTPS (PAM_TLS_CERT/PAM_TLS_KEY)", "PASS" if ok else "FAIL",
               f"HTTP {status}, request()->secure()={secure}, negotiated HTTP/{h2}")
    finally:
        app.start("--workers=2")


@check("Request timeout")
def timeout(app):
    app.start("--workers=2", server_env={"PAM_REQUEST_TIMEOUT_MS": "2000"})
    pids_before = {app.json("/_r/pid")[1]["pid"] for _ in range(20)}
    started = time.time()
    try:
        status, headers, _ = app.get("/_r/sleep/6000", timeout=15)
    except Exception as error:  # a dropped connection is a failure
        status, headers = type(error).__name__, {}
    elapsed = time.time() - started
    recovered = all(app.get("/_r/hello")[0] == 200 for _ in range(20))
    time.sleep(1.5)
    pids_after = {app.json("/_r/pid")[1]["pid"] for _ in range(20)}
    replaced = len(pids_before - pids_after) == 1
    ok = status == 504 and 1.9 < elapsed < 3.5 and recovered and replaced
    record("Request timeout answers 504 (2 s limit, blocking 6 s handler)", "PASS" if ok else "FAIL",
           f"HTTP {status} after {elapsed:.1f}s, connection={headers.get('connection')}, "
           f"20/20 healthy afterwards={recovered}, stuck worker replaced={replaced}")

    started = time.time()
    try:
        status, _, _ = stream_upload(app, "/_r/upload-sleep/6000", 100 * 1024 * 1024, timeout=30)
    except Exception as error:  # noqa: BLE001
        status = type(error).__name__
    leftovers = os.listdir(app.upload_dir)
    deadline = time.time() + 10
    while leftovers and time.time() < deadline:
        time.sleep(0.2)
        leftovers = os.listdir(app.upload_dir)
    ok = status == 504 and not leftovers
    record("Request timeout during a 100 MB upload leaves no temporary file", "PASS" if ok else "FAIL",
           f"HTTP {status} after {time.time() - started:.1f}s, temp files left={len(leftovers)}")
    app.start("--workers=2")


@check("Worker crash recovery")
def crashes(app):
    app.start("--workers=2")
    details = []
    ok = True
    for path in ["/_r/crash", "/_r/oom", "/_r/exit", "/_r/fatal"]:
        try:
            status = app.get(path, timeout=15)[0]
        except Exception as error:
            status = type(error).__name__
        time.sleep(0.5)
        healthy = 0
        deadline = time.time() + 15
        while time.time() < deadline and healthy < 20:
            try:
                healthy = healthy + 1 if app.get("/_r/hello", timeout=5)[0] == 200 else 0
            except Exception:
                healthy = 0
                time.sleep(0.2)
        ok = ok and healthy >= 20
        details.append(f"{path}->{status}, then {healthy}/20 ok")
    record("Worker crash recovery (SIGKILL, memory_limit fatal, exit(), exception)", "PASS" if ok else "FAIL",
           "; ".join(details))


@check("WorkerStopping lifecycle")
def worker_stopping(app):
    marker = os.path.join(app.dir, "storage", "readiness-stopping.log")
    app.stop(quiet=True)
    time.sleep(1)
    if os.path.exists(marker):
        os.unlink(marker)
    app.start("--workers=1")
    time.sleep(1)
    early = os.path.exists(marker) and open(marker).read().strip() != ""
    app.stop()
    time.sleep(1)
    late = os.path.exists(marker) and open(marker).read().strip() != ""
    ok = not early and late
    record("WorkerStopping fires on shutdown (not at boot)", "PASS" if ok else "FAIL",
           f"fired at boot={early}, fired at stop={late}")
    app.start("--workers=2")


def wrk(app, seconds, connections=64, path="/_r/hello"):
    proc = subprocess.run(["wrk", "-t4", f"-c{connections}", f"-d{seconds}s", "--latency", app.base + path],
                          capture_output=True, text=True, timeout=seconds + 60)
    out = proc.stdout
    requests = int(re.search(r"(\d+) requests in", out).group(1))
    errors = re.search(r"Socket errors: connect (\d+), read (\d+), write (\d+), timeout (\d+)", out)
    socket_errors = sum(int(value) for value in errors.groups()) if errors else 0
    non2xx = re.search(r"Non-2xx or 3xx responses: (\d+)", out)
    kinds = {name: int(value) for name, value in zip(("connect", "read", "write", "timeout"), errors.groups())
             if int(value)} if errors else {}
    return {"requests": requests, "socket_errors": socket_errors, "socket_error_kinds": kinds,
            "non_2xx": int(non2xx.group(1)) if non2xx else 0,
            "rps": float(re.search(r"Requests/sec:\s+([\d.]+)", out).group(1)), "raw": out}


@check("Zero-loss recycling and reload under wrk")
def zero_loss(app):
    if not shutil.which("wrk"):
        record("Zero-loss recycling and reload (wrk)", "N/A", "wrk is not installed")
        return
    seconds = int(os.environ.get("READINESS_ZERO_LOSS_SECONDS", "40"))
    app.start("--workers=4", "--max-requests=2000")
    pids = {app.json("/_r/pid")[1]["pid"] for _ in range(20)}
    stop = threading.Event()
    reloads = [0]

    def reloader():
        while not stop.wait(4):
            app.pam_cmd("octane:reload", check=True)
            reloads[0] += 1

    thread = threading.Thread(target=reloader)
    thread.start()
    try:
        result = wrk(app, seconds)
    finally:
        stop.set()
        thread.join()
    pids |= {app.json("/_r/pid")[1]["pid"] for _ in range(20)}
    lost = result["socket_errors"] + result["non_2xx"]
    ok = lost == 0 and result["requests"] >= 100_000 and reloads[0] >= 5
    record("Zero-loss: wrk -c64 keep-alive, --max-requests=2000 and a reload every 4 s", "PASS" if ok else "FAIL",
           f"{result['requests']} requests in {seconds}s ({result['rps']:.0f} rps), {reloads[0]} reloads, "
           f"{len(pids)}+ worker pids seen, socket errors={result['socket_errors']} {result['socket_error_kinds'] or ''}, "
           f"non-2xx={result['non_2xx']}")
    app.start("--workers=2")


@check("Edge page cache")
def edge_cache(app):
    """PAM's edge page cache with a real Laravel app: hits never reach PHP, a
    session response is never shared, and model changes purge every worker,
    from inside a worker and from another process (Artisan)."""
    app.start("--workers=3", server_env={"PAM_RESPONSE_CACHE": "1", "PAM_RESPONSE_CACHE_ALLOW_COOKIES": "1"})
    problems = []
    try:
        if app.get("/_r/cache/purge-all")[0] != 200:
            problems.append("purge-all route")
        labels = [app.get("/_r/view-cached")[1].get("x-pam-cache") for _ in range(30)]
        if labels.count("HIT") < 20:
            problems.append(f"view-cached labels {labels}")
        status, headers, body = app.get("/_r/view-cached", headers={"Cookie": "laravel_session=abc"})
        if status != 200 or headers.get("x-pam-cache") != "HIT" or "set-cookie" in headers or b"item-20" not in body:
            problems.append(f"view-cached with a session cookie: {status} {headers.get('x-pam-cache')}")
        if any(app.get("/_r/view")[1].get("x-pam-cache") == "HIT" for _ in range(10)):
            problems.append("an unmarked route was cached")

        sessions = set()
        for _ in range(15):
            status, headers, body = app.get("/_r/cache/web")
            session = json.loads(body).get("session")
            if headers.get("x-pam-cache") == "HIT" or "set-cookie" not in headers or session in sessions:
                problems.append(f"session page shared: {headers.get('x-pam-cache')} {session}")
                break
            sessions.add(session)

        def names(count=30):
            return {json.loads(app.get("/_r/cache/user/1")[2])["name"] for _ in range(count)}

        app.pam_cmd("artisan", "tinker", "--execute=DB::table('users')->where('id', 1)->update(['name' => 'before'])",
                    check=True)
        app.get("/_r/cache/rename/1?name=before")
        time.sleep(0.3)
        if names() != {"before"}:
            problems.append(f"warm names {names()}")
        # In-worker change: the trait purges this worker at once, siblings via the log.
        app.get("/_r/cache/rename/1?name=inside")
        time.sleep(0.3)
        seen = names(60)
        if seen != {"inside"}:
            problems.append(f"after in-worker purge: {seen}")
        # Out-of-worker change (Artisan): only the invalidation log reaches workers.
        app.pam_cmd("artisan", "tinker",
                    "--execute=ReadinessCachedUser::query()->findOrFail(1)->update(['name' => 'artisan'])", check=True)
        time.sleep(0.3)
        seen = names(60)
        if seen != {"artisan"}:
            problems.append(f"after Artisan purge: {seen}")
        app.pam_cmd("cache:purge", "--tag=users:1", check=True)
    finally:
        app.start("--workers=2")
    record("Edge page cache (hits, sessions never shared, model purges across workers and from Artisan)",
           "PASS" if not problems else "FAIL", "; ".join(problems) or "all correct")


def _decode(headers, data):
    encoding = headers.get("content-encoding", "")
    if encoding == "gzip":
        import gzip
        return gzip.decompress(data)
    if encoding == "br":
        import brotli  # noqa: PLC0415 (optional, only requested when installed)
        return brotli.decompress(data)
    return data


def _isolation_round(app, users, rounds, plan):
    """`users` threads, each its own identity, fire `rounds` requests picked from
    `plan` (path, credentials, expected-field) mixed with anonymous traffic on the
    same URLs. Every answer must carry the caller's identity (or none, for an
    anonymous caller); returns (problems, cache label counts)."""
    import random
    try:
        import brotli  # noqa: F401
        encodings = ["gzip", "br", "identity", None]
    except ImportError:
        encodings = ["gzip", "identity", None]
    problems, labels, lock = [], {}, threading.Lock()

    def one(uid, keep_alive):
        rnd = random.Random(uid)
        conn = None
        for _ in range(rounds):
            path, creds, field = rnd.choice(plan)
            anonymous = rnd.random() < 0.25
            headers = {}
            ident = None if anonymous else f"u{uid}"
            if ident and "cookie" in creds:
                headers["Cookie"] = f"uid={ident}"
            if ident and "bearer" in creds:
                headers["Authorization"] = f"Bearer {ident}"
            target = path + (f"?u={ident}" if ident and "query" in creds else "")
            if ident and "session" in creds:
                headers["Cookie"] = sessions[uid]
            encoding = rnd.choice(encodings)
            if encoding:
                headers["Accept-Encoding"] = encoding
            try:
                if conn is None:
                    conn = http.client.HTTPConnection("127.0.0.1", app.port, timeout=30)
                conn.request("GET", target, headers=headers)
                response = conn.getresponse()
                data = response.read()
                hdrs = {k.lower(): v for k, v in response.getheaders()}
                if not keep_alive:
                    conn.close()
                    conn = None
            except Exception as error:  # noqa: BLE001
                with lock:
                    problems.append(f"{target}: {type(error).__name__}")
                if conn is not None:
                    conn.close()
                conn = None
                continue
            body = json.loads(_decode(hdrs, data))
            label = hdrs.get("x-pam-cache", "none")
            with lock:
                labels[label] = labels.get(label, 0) + 1
                if response.status != 200 or body.get(field) != ident:
                    problems.append(f"{target} as {ident or 'anonymous'} got {field}={body.get(field)!r} "
                                    f"({response.status}, {label})")
        if conn is not None:
            conn.close()

    sessions = {}
    if any("session" in creds for _, creds, _ in plan):
        for uid in range(users):
            _, headers, _ = app.get(f"/_r/cache/login?u=u{uid}")
            sessions[uid] = headers["set-cookie"].split(";", 1)[0]
    threads = [threading.Thread(target=one, args=(uid, uid % 2 == 0)) for uid in range(users)]
    for thread in threads:
        thread.start()
    for thread in threads:
        thread.join()
    return problems, labels


@check("Edge page cache cross-user isolation")
def edge_cache_isolation(app):
    """Anti-leak battery: 48 concurrent users (cookie, bearer, query, Laravel
    session identities) mixed with anonymous callers, random Accept-Encoding,
    keep-alive and fresh connections, 3 workers, stale-while-revalidate
    refreshes in flight. No caller may ever see another caller's answer, with
    credentials refused (default) and with cookies/Authorization allowed."""
    report = []
    try:
        # Default: requests carrying credentials bypass the cache entirely, so
        # even a route that wrongly marks personal content public is safe.
        app.start("--workers=3", server_env={"PAM_RESPONSE_CACHE": "1"})
        app.get("/_r/cache/purge-all")
        plan = [("/_r/cache/me-cookie", {"cookie"}, "cookie"), ("/_r/cache/me-bearer", {"bearer"}, "bearer"),
                ("/_r/cache/me-query", {"query"}, "q"), ("/_r/cache/me-vary", {"cookie"}, "cookie"),
                ("/_r/cache/me-vary", {"bearer"}, "bearer"), ("/_r/cache/me-session", {"session"}, "session")]
        problems, labels = _isolation_round(app, 48, 120, plan)
        if labels.get("HIT", 0) < 100:
            problems.append(f"cache barely engaged: {labels}")
        report.append(("credentials refused", problems, labels))

        # Opt-in: cookies and Authorization allowed; only routes that declare
        # `Vary` on the credential (plus sessions, which set cookies) are used.
        app.start("--workers=3", server_env={"PAM_RESPONSE_CACHE": "1", "PAM_RESPONSE_CACHE_ALLOW_COOKIES": "1",
                                             "PAM_RESPONSE_CACHE_ALLOW_AUTHORIZATION": "1"})
        app.get("/_r/cache/purge-all")
        plan = [("/_r/cache/me-vary", {"cookie"}, "cookie"), ("/_r/cache/me-vary", {"bearer"}, "bearer"),
                ("/_r/cache/me-swr", {"cookie"}, "cookie"), ("/_r/cache/me-swr", {"bearer"}, "bearer"),
                ("/_r/cache/me-query", {"query"}, "q"), ("/_r/cache/me-session", {"session"}, "session")]
        problems, labels = [], {}
        for _ in range(3):  # spans several 1 s TTLs of me-swr, so STALE + revalidation run under load
            more, counted = _isolation_round(app, 48, 80, plan)
            problems += more
            for key, value in counted.items():
                labels[key] = labels.get(key, 0) + value
            time.sleep(1.2)
        if labels.get("HIT", 0) < 500 or labels.get("STALE", 0) < 1:
            problems.append(f"cache barely engaged: {labels}")
        report.append(("cookies + Authorization allowed", problems, labels))
    finally:
        app.start("--workers=2")
    failed = [f"{name}: {'; '.join(sorted(set(p))[:4])}" for name, p, _ in report if p]
    detail = "; ".join(f"{name}: {sum(l.values())} requests {l}" for name, _, l in report)
    record("Edge page cache cross-user isolation (48 users, cookie/bearer/query/session, gzip/br, keep-alive, SWR)",
           "PASS" if not failed else "FAIL", " | ".join(failed) or detail)


CHECKS = [basics, isolation, concurrently, swoole_only, streaming, static_files, keepalive, proxies, uploads,
          memory, max_requests, reload, zero_loss, lifecycle, timeout, crashes, worker_stopping, https, watch,
          edge_cache, edge_cache_isolation]


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--app", required=True)
    parser.add_argument("--pam", default="pam")
    parser.add_argument("--port", type=int, default=8100)
    parser.add_argument("--out")
    parser.add_argument("--only", nargs="*")
    args = parser.parse_args()
    app = App(args)
    app.start("--workers=2")
    try:
        for fn in CHECKS:
            if args.only and fn.__name__ not in args.only:
                continue
            fn(app)
    finally:
        app.stop(quiet=True)
    if args.out:
        with open(args.out, "w") as handle:
            json.dump(RESULTS, handle, indent=2)
    failed = [r for r in RESULTS if r["status"] == "FAIL"]
    print(f"\n{len(RESULTS)} checks, {len(failed)} failed")
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
