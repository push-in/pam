# Performance offensive

PAM's persistent worker path is intentionally small. Request bodies transfer
ownership into the PHP executor instead of being copied; worker generations,
bounded queues and backpressure preserve predictable latency under load.

## Optimized builds

```bash
scripts/build-performance.sh release
scripts/build-performance.sh pgo
scripts/build-performance.sh bolt
```

`release` uses fat LTO and one codegen unit. `pgo` builds an instrumented PAM,
trains it with canonical CLI/runtime work, merges profiles with
`llvm-profdata`, and rebuilds from that evidence. `bolt` is available when
Linux x86_64 and LLVM BOLT tools are installed; it reorders the linked binary
from an instrumented training run. The dedicated performance profile retains
line tables for `perf`, Instruments, and flamegraphs.

## Regression evidence

The checked-in benchmark suites report p50, p95, p99, throughput, error rate,
startup/recovery latency, and memory. Run the HTTP matrix and soak protocol from
[`benchmarks/README.md`](../benchmarks/README.md); process-manager recovery has
its own reproducible harness under `benchmarks/process-manager`.

Compare identical hardware, PHP runtime, worker count, connection policy,
payload, warmup, sample count, and thermal state. Never accept an average-only
win that makes p99, errors, or resident memory worse. Pull requests retain the
functional contracts; evidence workflows run the heavier load and soak gates
and publish their raw artifacts.
