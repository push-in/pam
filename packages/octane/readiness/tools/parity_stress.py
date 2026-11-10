#!/usr/bin/env python3
"""Concurrency and chaos checks for PAM's Swoole-parity features.

Runs against a server started with task workers (the readiness app's probe
routes) and checks, one scenario at a time:

  writers      N parallel clients increment one shared-table row: exact total
  misses       N parallel cache misses on fresh keys and on one key: all 200
  disconnects  clients drop the connection while Octane::concurrently() waits
  slow         slowloris-style clients while normal traffic must not fail
  oom          an HTTP worker and a task worker exceed memory_limit
  health       after everything: hello, concurrently and table still work

  parity_stress.py --port 8160 [--concurrency 256]

Prints one JSON object (also written to --out); exit status 1 on any failure.
"""
import argparse
import asyncio
import json
import random
import sys
import time


async def request(port, path, timeout=30):
    reader, writer = await asyncio.wait_for(asyncio.open_connection("127.0.0.1", port), timeout)
    try:
        writer.write(f"GET {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n".encode())
        await writer.drain()
        data = await asyncio.wait_for(reader.read(), timeout)
    finally:
        writer.close()
    head, _, body = data.partition(b"\r\n\r\n")
    status = int(head.split(b" ", 2)[1])
    if b"transfer-encoding: chunked" in head.lower():
        decoded, rest = b"", body
        while rest:
            size_line, _, rest = rest.partition(b"\r\n")
            size = int(size_line, 16)
            if size == 0:
                break
            decoded, rest = decoded + rest[:size], rest[size + 2:]
        body = decoded
    return status, body


async def json_get(port, path, timeout=30):
    status, body = await request(port, path, timeout)
    return status, (json.loads(body) if status == 200 else body[:200].decode(errors="replace"))


async def gather_limited(coros, limit):
    semaphore = asyncio.Semaphore(limit)

    async def run(coro):
        async with semaphore:
            try:
                return await coro
            except Exception as error:  # noqa: BLE001
                return error

    return await asyncio.gather(*(run(c) for c in coros))


async def writers(port, concurrency, per_client):
    key = f"stress-{random.getrandbits(32)}"
    total = concurrency * per_client
    results = await gather_limited([json_get(port, f"/_r/parity/table/incr?key={key}") for _ in range(total)], concurrency)
    failed = [r for r in results if isinstance(r, Exception) or r[0] != 200]
    _, row = await json_get(port, f"/_r/parity/table/get?key={key}")
    votes = (row["row"] or {}).get("votes", 0)
    seen = sorted(r[1]["votes"] for r in results if not isinstance(r, Exception) and r[0] == 200)
    return {"sent": total, "failed": len(failed), "final": votes,
            "distinct_results": len(set(seen)) == len(seen), "ok": not failed and votes == total and seen == list(range(1, total + 1))}


async def misses(port, concurrency):
    fresh = await gather_limited([json_get(port, f"/_r/parity/cache/remember?key=m{random.getrandbits(40)}&bytes=2048") for _ in range(concurrency * 4)], concurrency)
    same_key = f"same{random.getrandbits(32)}"
    same = await gather_limited([json_get(port, f"/_r/parity/cache/remember?key={same_key}&bytes=4096") for _ in range(concurrency * 4)], concurrency)
    bad = [r for r in fresh + same if isinstance(r, Exception) or r[0] != 200]
    ok = not bad and {r[1]["length"] for r in fresh} == {2048} and {r[1]["length"] for r in same} == {4096}
    return {"fresh": len(fresh), "same_key": len(same), "failed": len(bad), "ok": ok}


async def disconnects(port, count):
    async def drop():
        reader, writer = await asyncio.open_connection("127.0.0.1", port)
        writer.write(b"GET /_r/parity/concurrently?sleep=300&count=2&wait=5000 HTTP/1.1\r\nHost: localhost\r\n\r\n")
        await writer.drain()
        await asyncio.sleep(random.uniform(0.0, 0.25))
        writer.transport.abort()

    started = time.time()
    await gather_limited([drop() for _ in range(count)], 64)
    # Abandoned tasks finish on their own; the pool must then be free again.
    await asyncio.sleep(2)
    status, body = await json_get(port, "/_r/parity/concurrently?sleep=200&count=2&wait=5000")
    healthy = await gather_limited([request(port, "/_r/hello") for _ in range(100)], 16)
    bad = [r for r in healthy if isinstance(r, Exception) or r[0] != 200]
    ok = status == 200 and len(body["pids"]) == 2 and all(isinstance(p, int) for p in body["pids"]) and not bad
    return {"dropped": count, "seconds": round(time.time() - started, 1), "concurrently_after": body, "hello_failed": len(bad), "ok": ok}


async def slow(port, slow_clients, seconds):
    async def loris():
        try:
            reader, writer = await asyncio.open_connection("127.0.0.1", port)
            for byte in b"GET /_r/hello HTTP/1.1\r\nHost: localhost\r\nX-Slow: " + b"a" * 4000:
                writer.write(bytes([byte]))
                await writer.drain()
                await asyncio.sleep(0.2)
        except Exception:  # noqa: BLE001
            pass  # The server may close a client that is too slow.

    tasks = [asyncio.create_task(loris()) for _ in range(slow_clients)]
    deadline = time.time() + seconds
    sent = failed = 0
    while time.time() < deadline:
        results = await gather_limited([request(port, "/_r/hello", 10) for _ in range(32)], 16)
        sent += len(results)
        failed += sum(1 for r in results if isinstance(r, Exception) or r[0] != 200)
        tasks_result = await json_get(port, "/_r/parity/concurrently?sleep=0&count=2&wait=5000")
        sent += 1
        failed += tasks_result[0] != 200
    for task in tasks:
        task.cancel()
    return {"slow_clients": slow_clients, "sent": sent, "failed": failed, "ok": failed == 0}


async def oom(port):
    http = await request(port, "/_r/oom")
    task = await json_get(port, "/_r/parity/task-oom", 60)
    await asyncio.sleep(1)
    after = await gather_limited([json_get(port, "/_r/parity/concurrently?sleep=0&count=4&wait=5000") for _ in range(20)], 4)
    bad = [r for r in after if isinstance(r, Exception) or r[0] != 200]
    ok = http[0] >= 500 and task[0] == 200 and task[1]["failed"] is not None and not bad
    return {"http_status": http[0], "task": task[1], "after_failed": len(bad), "ok": ok}


async def health(port):
    hello = await gather_limited([request(port, "/_r/hello") for _ in range(200)], 32)
    tasks = await json_get(port, "/_r/parity/concurrently?sleep=300&count=2&wait=5000")
    count = await json_get(port, "/_r/parity/table/count")
    bad = [r for r in hello if isinstance(r, Exception) or r[0] != 200]
    ok = not bad and tasks[0] == 200 and tasks[1]["ms"] < 1000 and count[0] == 200
    return {"hello_failed": len(bad), "concurrently_ms": tasks[1].get("ms") if tasks[0] == 200 else tasks, "ok": ok}


async def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--port", type=int, default=8160)
    parser.add_argument("--concurrency", type=int, default=256)
    parser.add_argument("--rounds", type=int, default=3)
    parser.add_argument("--out")
    args = parser.parse_args()
    report = {"rounds": []}
    for _ in range(args.rounds):
        report["rounds"].append({
            "writers": await writers(args.port, args.concurrency, 20),
            "misses": await misses(args.port, args.concurrency),
            "disconnects": await disconnects(args.port, 500),
            "slow": await slow(args.port, 200, 20),
            "oom": await oom(args.port),
            "health": await health(args.port),
        })
    report["ok"] = all(item["ok"] for round_ in report["rounds"] for item in round_.values())
    text = json.dumps(report, indent=2)
    if args.out:
        with open(args.out, "w") as handle:
            handle.write(text)
    print(text)
    return 0 if report["ok"] else 1


if __name__ == "__main__":
    sys.exit(asyncio.run(main()))
