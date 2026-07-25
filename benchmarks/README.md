# PAM Laravel benchmark laboratory

Performance claims must compare the same application rather than empty handlers
from unrelated stacks. The executable laboratory in `benchmarks/laravel`
compares PAM, PHP-FPM + Nginx, Laravel Octane + Swoole, FrankenPHP and
RoadRunner.

Run the complete pinned matrix:

```bash
scripts/benchmark-laravel.sh
```

It builds every container, starts only one runtime at a time, warms it, executes
three measured rounds and writes raw and aggregated JSON under
`benchmarks/results/<UTC timestamp>`. Override the workload without editing the
protocol:

```bash
PAM_BENCH_ROUNDS=5 \
PAM_BENCH_WARMUP_SECONDS=20 \
PAM_BENCH_DURATION_SECONDS=60 \
PAM_BENCH_THREADS=4 \
PAM_BENCH_CONNECTIONS=128 \
PAM_BENCH_ENDPOINT=/api/ping \
scripts/benchmark-laravel.sh
```

## Fixed inputs

- same machine, kernel, CPU governor and open-file limits;
- same Laravel release, `composer.lock`, route code, environment and response bytes;
- production dependencies with optimized authoritative autoloading;
- four application workers, two application CPUs and a 1 GiB memory contract;
- no Xdebug, debug pages, access logs, tracing or profilers;
- same HTTP version and TLS mode;
- local load generator on a separate recorded CPU set when at least four CPUs
  are available.

Containers enforce the CPU and memory limits directly. PAM is pinned to the
recorded application CPU set and the run fails if the aggregate RSS of its
master and workers exceeds 1 GiB.

The default contract endpoint is `GET /api/ping` in `compat/laravel-smoke`.
PAM is started in production cluster mode by the script:

```bash
pam start compat/laravel-smoke/pam.php \
  --workers 4 \
  --admin-address 127.0.0.1:19084
```

The vendored `wrk.lua` emits machine-readable requests/second, p50/p75/p90/p95/
p99/max latency and all connection, read, write, timeout and HTTP status errors.
The runner also captures `docker stats`, host metadata and the exact Git commit.
Repeat at multiple concurrency levels; the best throughput number is not useful
if tail latency or errors collapse.

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
