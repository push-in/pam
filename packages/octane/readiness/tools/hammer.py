#!/usr/bin/env python3
"""Closed-loop HTTP/1.1 load generator that counts every failed request.

wrk and ab are poor at *counting* failures (ab aborts on the first reset, wrk
reports socket errors from reconnect loops). This tool sends `--requests`
requests (or runs for `--seconds`) with `--concurrency` loops and classifies
each one, so "zero dropped requests" claims are measurable.

  hammer.py http://127.0.0.1:8000/_r/hello -c 8 -s 10 [--keepalive]
Prints one JSON object.
"""
import argparse
import asyncio
import json
import time
import urllib.parse


async def one_request(reader, writer, request):
    writer.write(request)
    await writer.drain()
    head = await reader.readuntil(b"\r\n\r\n")
    status_line, _, header_blob = head.partition(b"\r\n")
    status = int(status_line.split()[1])
    headers = {}
    for line in header_blob.split(b"\r\n"):
        if b":" in line:
            key, _, value = line.partition(b":")
            headers[key.strip().lower()] = value.strip()
    if b"content-length" in headers:
        await reader.readexactly(int(headers[b"content-length"]))
    elif headers.get(b"transfer-encoding", b"").lower() == b"chunked":
        while True:
            size = int((await reader.readuntil(b"\r\n")).strip().split(b";")[0], 16)
            await reader.readexactly(size + 2)
            if size == 0:
                break
    else:
        await reader.read()
    return status, headers.get(b"connection", b"").lower() == b"close"


async def loop(args, url, deadline, counter, stats, latencies):
    host, port = url.hostname, url.port or 80
    path = url.path or "/"
    if url.query:
        path += "?" + url.query
    conn_header = "keep-alive" if args.keepalive else "close"
    request = (
        f"GET {path} HTTP/1.1\r\nHost: {host}:{port}\r\n"
        f"Connection: {conn_header}\r\nUser-Agent: pam-readiness-hammer\r\n\r\n"
    ).encode()
    reader = writer = None
    while True:
        if args.seconds and time.monotonic() >= deadline:
            break
        if not args.seconds:
            if counter[0] >= args.requests:
                break
            counter[0] += 1
        started = time.perf_counter()
        try:
            if writer is None:
                reader, writer = await asyncio.wait_for(
                    asyncio.open_connection(host, port), args.timeout
                )
            status, close = await asyncio.wait_for(
                one_request(reader, writer, request), args.timeout
            )
            latencies.append(time.perf_counter() - started)
            key = "2xx" if 200 <= status < 300 else str(status)
            stats[key] = stats.get(key, 0) + 1
            if close or not args.keepalive:
                writer.close()
                writer = None
        except Exception as error:  # noqa: BLE001 - every failure is data
            name = type(error).__name__
            stats["error:" + name] = stats.get("error:" + name, 0) + 1
            if writer is not None:
                writer.close()
            writer = None
    if writer is not None:
        writer.close()


async def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("url")
    parser.add_argument("-c", "--concurrency", type=int, default=8)
    parser.add_argument("-n", "--requests", type=int, default=1000)
    parser.add_argument("-s", "--seconds", type=float, default=0)
    parser.add_argument("-t", "--timeout", type=float, default=10)
    parser.add_argument("--keepalive", action="store_true")
    args = parser.parse_args()
    url = urllib.parse.urlparse(args.url)
    stats, latencies, counter = {}, [], [0]
    started = time.monotonic()
    deadline = started + args.seconds
    await asyncio.gather(
        *(loop(args, url, deadline, counter, stats, latencies) for _ in range(args.concurrency))
    )
    elapsed = time.monotonic() - started
    latencies.sort()

    def pct(p):
        return round(latencies[min(len(latencies) - 1, int(len(latencies) * p))] * 1000, 2) if latencies else None

    total = sum(stats.values())
    failed = total - stats.get("2xx", 0)
    print(json.dumps({
        "url": args.url, "keepalive": args.keepalive, "concurrency": args.concurrency,
        "total": total, "ok": stats.get("2xx", 0), "failed": failed, "breakdown": stats,
        "rps": round(total / elapsed, 1) if elapsed else None,
        "p50_ms": pct(0.50), "p99_ms": pct(0.99),
    }))


if __name__ == "__main__":
    asyncio.run(main())
