# Cross-surface observability

PAM uses one trace lineage and explicit privacy boundaries across Server,
Native, and Desktop. The surfaces do not pretend to have equal evidence:
Server exports online protocol spans today, while Native and Desktop retain
bounded specialized snapshots until their online mappings are certified.

## Interoperability status

| Surface | Live data | Persistent evidence | OTLP status |
| --- | --- | --- | --- |
| Server | Prometheus metrics, structured logs, W3C trace context | Redacted Chrome/Perfetto timeline | OTLP/HTTP JSON traces implemented and certified against the official Collector |
| Native | Navigation metrics/timeline and the optional observability package | Schema 1 snapshot, surface code `2` | Package transport currently uses its vendor-neutral schema; an OTLP claim is intentionally withheld |
| Desktop | Authenticated aggregate command/worker diagnostics | Schema 1 snapshot, surface code `3` | Aggregate-only; individual command spans are not fabricated |

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

## Online architecture target

The next protocol revision must preserve these boundaries:

```text
Server HTTP request ───────────────► OTLP trace receiver
Native explicit app spans ─adapter─► OTLP trace receiver
Native logs/metrics ───────adapter─► matching OTLP signal endpoint
Desktop aggregate snapshot ────────► local timeline/evidence only
Desktop future command spans ─gate─► OTLP only with explicit production opt-in
```

- Server remains non-blocking with bounded batch and queue controls.
- Native adapters must translate each signal to the matching OTLP schema; the
  existing custom batch cannot be relabeled as OTLP.
- Native application context remains explicit and allowlisted. Crash details,
  paths and exception messages require a separately documented consent policy.
- Desktop production export defaults off. The development bridge token,
  application arguments, filesystem paths and command payloads are never span
  attributes.
- Cross-surface trace IDs may be propagated only through authenticated product
  channels. Snapshots and offline timelines never invent missing trace IDs or
  timestamps.

Collector upgrades require a new digest, successful signature verification,
the full certification suite and a fresh evidence artifact. Compatibility
against one version is evidence for that immutable version, not a claim about
every future Collector.
