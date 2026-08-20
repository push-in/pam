# Pam benchmark protocol

Performance claims must compare the same application rather than empty handlers
from unrelated stacks. Use this protocol for Pam, FrankenPHP and Swoole.

## Fixed inputs

- same machine, kernel, CPU governor and open-file limits;
- same Laravel release, `composer.lock`, route code, `.env` and response bytes;
- production dependencies with optimized authoritative autoloading;
- same number of PHP workers and CPU affinity;
- no Xdebug, debug pages, access logs, tracing or profilers;
- same HTTP version and TLS mode;
- local load generator on a separate CPU set when possible.

The repository contract endpoint is `GET /api/ping` in
`compat/laravel-smoke`. Start Pam in production mode:

```bash
pam start compat/laravel-smoke/pam.php \
  --workers 10 \
  --max-requests 1000000 \
  --admin-address 127.0.0.1:3010
```

Warm the application before recording results:

```bash
wrk -t2 -c32 -d10s http://127.0.0.1:31310/api/ping
```

Then run at least three measured rounds:

```bash
wrk -t4 -c1000 -d30s --latency http://127.0.0.1:31310/api/ping
```

Record requests/second, average latency, p50/p95/p99, errors, worker RSS,
event-loop lag and CPU utilization. Repeat at multiple concurrency levels; the
lag evidence should retain current, maximum, and sample-weighted average values
per worker and pool so one stalled process cannot disappear inside an aggregate.
The best throughput number is not useful if tail latency or errors collapse.

## Memory stability

The Laravel integration test warms the worker, verifies request-container
isolation, sends another 2,000 requests and enforces bounded RSS growth:

```bash
cargo test --test laravel -- --nocapture
```

For a release candidate, extend the soak to millions of requests with the real
database, auth, logging and package set. Set `--max-requests` from measured RSS,
not from an arbitrary benchmark-friendly value.

Do not publish “faster than” conclusions unless raw commands, versions, hardware,
configuration and every result round are included.
