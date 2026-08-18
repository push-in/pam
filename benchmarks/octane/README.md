# PAM Octane benchmark lab

This lab runs the same Laravel fixture with one application worker on PAM,
FrankenPHP and OpenSwoole. Images are pinned by digest. `wrk` receives identical
thread, connection, duration and route settings.

```bash
PAM_BENCH_ROUNDS=3 PAM_BENCH_DURATION=15s benchmarks/octane/run.sh
```

The release matrix runs the same suite with several equal worker counts. It
requires PAM to retain at least 80% of FrankenPHP's median dynamic throughput
at every point while keeping dynamic p99 below 100 ms:

```bash
PAM_BENCH_ROUNDS=5 \
PAM_BENCH_DURATION=30s \
PAM_BENCH_WORKER_MATRIX="1 4 8" \
benchmarks/octane/matrix.sh
```

Release evidence requires at least five rounds per scenario. Each round rotates
the scenario order, performs a fresh warmup, and retains the raw output. The
gate also rejects a relative median absolute deviation above 5%, any transport
or HTTP error, or a PAM dynamic p99 above 100 ms. A one-round run is calibration
only and can never pass the measurement gate. Override the two release floors
only for stricter private environments with `PAM_BENCH_MIN_DYNAMIC_RATIO` and
`PAM_BENCH_MAX_DYNAMIC_P99_US`; published PAM evidence uses the defaults.

`metadata.json` records the exact source commit and dirty state, PAM binary
hash, pinned container digest, host CPU/kernel/governor, load and every workload
parameter. Results from a dirty tree are useful during development but are not
publishable release evidence.

The report deliberately separates two claims:

- `uncached`, `blade`, `database` and `large-json`: every request executes the
  Laravel Kernel, covering minimal JSON, compiled Blade, SQLite through the
  query builder and a larger serialization workload;
- `edge-cache`: PAM's explicitly configured native public-response cache is
  compared with full Laravel execution on runtimes without that feature.

The 5x gate applies only to the named edge-cache scenario. It must never be
reported as a 5x improvement for arbitrary dynamic Laravel requests. Raw `wrk`
output, latency percentiles, errors, server logs and the aggregate JSON are kept
under `benchmarks/octane/results` locally and ignored by Git.

The runner also samples the complete process tree for RSS, CPU and process
count. `resources.json` is evidence alongside throughput, not a replacement for
it. Any socket error, timeout or non-2xx response fails the zero-error gate.

Every comparison, worker matrix, and soak run also creates an
`evidence-manifest.json`. The manifest records the sequential integer suite ID,
source metadata, parameters, gate outcome, byte size, and SHA-256 digest of every
JSON, CSV, log, and raw text artifact. The runner verifies the manifest before a
run can finish successfully:

```bash
php benchmarks/octane/evidence-manifest.php \
  benchmarks/octane/results 1 --verify
```

Suite IDs are `1` for a comparison, `2` for a worker matrix, and `3` for a soak.
