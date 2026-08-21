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

## Process-manager recovery

`benchmarks/process-manager/run.sh` measures full master recovery from
`SIGKILL`, from signal dispatch until a different ready PID is observable. It
uses an isolated manager state/runtime directory, resets recovery state before
each round, records every latency in CSV, and gates 100% successful recovery,
p95 latency, and daemon RSS growth.

```bash
PAM_RECOVERY_ROUNDS=10 \
PAM_RECOVERY_MAX_P95_MILLIS=200 \
PAM_RECOVERY_MAX_DETECTION_P95_MILLIS=10 \
PAM_RECOVERY_MAX_BACKOFF_P95_MILLIS=20 \
PAM_RECOVERY_MAX_READINESS_P95_MILLIS=150 \
PAM_RECOVERY_MAX_RSS_GROWTH_BYTES=16777216 \
benchmarks/process-manager/run.sh
```

The default output is ignored under `benchmarks/process-manager/results/` and
contains metadata, raw measurements, resource evidence, a schema-1 report and
a suite-5 SHA-256 manifest. The runner refuses to overwrite evidence. Set
`PAM_RECOVERY_RESULTS` to a new directory and `PAM_BENCH_BINARY` to the exact
candidate binary for release measurements.

Each round also records `worker-startup.csv`: worker count, first-to-last spawn
spread, and the generation's p95/maximum spawn-to-ready latency. These values
come from worker-published timestamps carried through the ready master state,
not from external CLI polling. The report aggregates them separately from the
existing end-to-end and recovery-phase metrics.

Set `PAM_RECOVERY_WORKERS` from 1 through 64 to measure a bounded worker count.
Suite 7 fixes the release matrix at 1, 4 and 16 workers and runs the complete
suite-5 protocol independently for each configuration:

```bash
PAM_RECOVERY_MATRIX_ROUNDS=10 \
benchmarks/process-manager/worker-matrix.sh
```

Every configuration must recover all rounds and pass its latency, phase and RSS
gates. Total/readiness p95 limits are respectively 200/150 ms for 1 worker,
250/200 ms for 4 workers and 650/550 ms for 16 workers; detection, effective
backoff and RSS limits remain 10 ms, 20 ms and 16 MiB. The aggregate additionally
rejects missing or extra configurations, unequal round counts and differing
source, binary, PAM Native or host provenance. Its recursive suite-7 manifest
binds the aggregate plus all raw per-configuration artifacts.
Reports and manifests are still generated and uploaded when a performance gate
fails, while the runner preserves the nonzero exit status.
