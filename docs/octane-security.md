# PAM Octane security review

## Trust boundaries

The public HTTP listener is untrusted. Laravel/PHP code is trusted application
code. The supervisor control plane and cache purge endpoint are operational
interfaces and must remain private. Child processes, upstream HTTP servers,
Redis and databases are separate trust boundaries.

## Enforced controls

- Request bodies, headers, header count, response bodies, response chunks,
  WebSocket messages, queues and active connections are bounded.
- Header values are parsed by `http`; response headers are reconstructed using
  typed names and values, preventing CRLF injection.
- Request timeouts cancel suspended PHP Fibers and native operations. Worker
  watchdog deadlines terminate PHP that cannot cooperate.
- The PHP executor queue is bounded independently from active PHP concurrency;
  overload returns 503 with `Retry-After` instead of growing memory indefinitely.
- Response cache keys are SHA-256 digests. Authorization and cookies bypass the
  cache; private, no-store and Set-Cookie responses are never stored. Sensitive
  headers are forbidden in the configured Vary list.
- Cache purge requires POST and a secret of at least 32 bytes. Token digests are
  compared without an early exit. Tag syntax and count are bounded. The
  invalidation log is created inside the supervisor's mode-0700 runtime
  directory and workers open it with mode 0600.
- Master state is written atomically with mode 0600. Symlinks are refused and
  PID start-time fingerprints prevent signalling a reused PID.
- Upload filenames are reduced to basenames, temporary files are deleted at the
  request boundary, and body/response limits apply to both buffered and streamed
  traffic.
- HTTP redirects drop Authorization when the destination host changes. TLS peer
  verification is enabled by default.
- Redis RESP nesting, arrays, bulk values, buffers and deadlines are bounded;
  protocol errors close the connection.
- Isolated PDO credentials and SQL travel over child stdin rather than argv.
  Worker count, waiting queue, runtime and output are bounded.
- Process commands use argument arrays without a shell. Timeout escalation
  targets the dedicated process group.
- Route metrics use Laravel route templates and enforce a fixed cardinality
  ceiling. The internal route header is removed before transmission.

## Verification

Run:

```bash
cargo test
php packages/octane/vendor/bin/phpunit --configuration packages/octane/phpunit.xml
cargo test protocol_tests
PAM_SOAK_DURATION=30m benchmarks/octane/soak.sh
```

The scheduled security workflow audits locked Rust and Composer dependencies,
runs memory/Laravel contracts, executes native shutdown under Valgrind, and
runs a time-bounded, coverage-guided libFuzzer campaign against the exact
dispatch-envelope parser used in production. A deterministic malformed corpus
also runs in the ordinary Rust suite for fast regression feedback. HTTP, Redis
and isolated PDO integration tests verify that slow I/O does not block another
request.

Run the fuzzer locally with nightly Rust and `cargo-fuzz` 0.13.2:

```bash
cargo fuzz run dispatch_envelope -- -max_total_time=120 -timeout=5
```

## Residual risks

- Embedded PHP remains memory-unsafe native code; process workers and recycling
  are the containment boundary for extension defects.
- Arbitrary application code can block without yielding. The watchdog recovers
  the worker but cannot make that code cooperative.
- `IsolatedPdoPool` is intentionally process-isolated and has higher latency than
  a persistent native driver. It does not provide cross-query transactions.
- The cache invalidation log is scoped to the supervisor lifetime. A full
  restart starts with empty worker caches, so persistence is unnecessary.
- The application must keep the supervisor control address private and supply
  secrets through its deployment secret manager.
