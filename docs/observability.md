# Cross-surface observability

PAM uses one trace lineage and explicit privacy boundaries across Server,
Native, and Desktop. Each online mapping remains explicit, privacy-bounded and
independently certified while specialized snapshots preserve local tooling.

## Interoperability status

| Surface | Live data | Persistent evidence | OTLP status |
| --- | --- | --- | --- |
| Server | Prometheus metrics, structured logs, W3C trace context | Redacted Chrome/Perfetto timeline | OTLP/HTTP JSON traces implemented and certified against the official Collector |
| Native | Navigation metrics/timeline and the optional observability package | Schema 1 snapshot, surface code `2` | OTLP/HTTP JSON traces, logs, delta counters, and gauges certified by the package; PAM JSON remains the compatibility default |
| Desktop | Authenticated aggregate command/worker diagnostics | Schema 1 snapshot, surface code `3` | Explicit-opt-in OTLP/HTTP JSON root spans for validated commands, certified against the official Collector |

This distinction is a compatibility promise. A JSON endpoint is not called
OTLP merely because it accepts telemetry-shaped data. PAM publishes the claim
only after the official Collector accepts the exact wire representation.

## Reproduce the Collector certification

The certification starts a PAM fixture and the official OpenTelemetry
Collector core distribution, sends a sampled W3C child trace, and verifies four
independent gates:

```bash
scripts/certify-otlp.sh
python3 scripts/otlp-evidence.py artifacts/otlp 1 --verify
```

The Collector `0.157.0` image is fixed by its multi-platform digest. CI also
verifies its Sigstore identity before execution. The container binds OTLP only
to loopback, has a read-only root/configuration, drops Linux capabilities,
forbids privilege escalation, and is removed at exit. A
locally downloaded image and temporary Cargo target are removed when the script
created them.

Evidence suite `1` proves:

1. the official Collector accepted PAM's OTLP/HTTP JSON payload;
2. the caller's W3C parent span remained the server span parent;
3. `service.name` identified the emitting service;
4. a deliberately sensitive query value did not reach the Collector.

The evidence manifest accepts only four named regular files, caps each at
2 MiB, records SHA-256 and refuses failed or modified evidence. CI publishes
the bounded directory for 30 days; local evidence under `artifacts/` stays out
of Git. Publishable manifests require a clean worktree. During harness
development, `PAM_OTLP_ALLOW_DIRTY=1` permits a clearly marked local diagnostic
manifest, but CI never uses that escape hatch.

PAM Desktop has an independent acceptance harness in its repository:

```bash
scripts/certify-desktop-otlp.sh
```

It runs an ignored Rust integration test against the same immutable Collector,
checks the exported span and static command name in Collector output, and
removes its container, downloaded image and temporary Cargo target at exit. CI
verifies the Collector's Sigstore identity before running the harness.

## Online architecture and remaining boundary

The next protocol revision must preserve these boundaries:

```text
Server HTTP request ───────────────► OTLP trace receiver
Native explicit app spans ─adapter─► OTLP trace receiver       ✓
Native logs/metrics ───────adapter─► matching OTLP endpoint    ✓
Desktop aggregate snapshot ────────► local diagnostics
Desktop validated commands ──gate──► OTLP trace receiver        ✓
Server response traceparent ──valid─► Native child span          ✓
Server response traceparent ──auth──► Desktop command child      ✓
```

- Server remains non-blocking with bounded batch and queue controls.
- Native adapters translate each signal to the matching OTLP schema; they do
  not relabel the existing custom batch. `WireProtocol` keeps both modes
  explicit with sequential integer codes.
- Native application context remains explicit and allowlisted. Crash details,
  paths and exception messages require a separately documented consent policy.
- Desktop production export defaults off. The development bridge token,
  application arguments, filesystem paths and command payloads are never span
  attributes. Its bounded queue cannot stall command execution, and diagnostic
  counters expose dropped, failed and Collector-rejected spans.
- Cross-surface trace IDs may be propagated only through authenticated product
  channels. Native's `TraceContext` strictly imports a Server response context,
  preserves sampling and refuses invalid/zero IDs (`622df52`). Desktop accepts
  that context only inside its exact-origin, ephemeral-token bridge invocation,
  validates it again in Rust and preserves parent lineage (`d93c63b`). Neither
  surface accepts `tracestate` until a bounded vendor policy exists. Snapshots
  and offline timelines never invent missing trace IDs or timestamps.

Collector upgrades require a new digest, successful signature verification,
the full certification suite and a fresh evidence artifact. Compatibility
against one version is evidence for that immutable version, not a claim about
every future Collector.
