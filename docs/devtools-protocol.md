# DevTools snapshot protocol

PAM diagnostics use one additive envelope across server, Native, and Desktop.
The envelope lets tooling route snapshots without guessing from payload shape;
each surface retains its specialized metrics and inspection data.

```json
{
  "schemaVersion": 1,
  "surfaceCode": 1,
  "capturedAtUnixMs": 1787068123678
}
```

Surface codes are sequential integer enum values:

| Code | Surface | Current snapshot payload |
| ---: | --- | --- |
| 1 | Server | Memory, fibers, resources, leaks, profiles, events, connections |
| 2 | Native | Navigation state tree, performance metrics, bounded timeline |
| 3 | Desktop | Command metrics, worker generations, pool size, event cursor |

Snapshot access follows the discovered project surface:

| Surface | Developer access |
| --- | --- |
| Server | `pam diagnostics [script]` |
| Native | `pam diagnostics` on Android debug or an iOS debug simulator |
| Desktop | `pam diagnostics` while that project is running under `pam dev` |

Desktop delegates to its authenticated loopback gateway through a bounded,
ephemeral project descriptor. The unified CLI never reads the bridge token into
its own process; it only invokes the matching Desktop host command.

The three envelope fields are required. Surface-specific fields remain at the
top level in schema 1 for compatibility with existing consumers. Native also
retains its legacy payload `version` while clients migrate to `schemaVersion`.

Consumers must ignore unknown fields. Adding optional metrics is compatible;
changing an existing field's meaning, integer unit, or shape requires a new
`schemaVersion`. Timestamps use Unix epoch milliseconds. Timelines remain
bounded at their source, and exporters must redact credentials, cookies,
authorization values, and application secrets before persistence.

The live development lifecycle is a separate streaming contract documented in
`development-events.md`; snapshots describe current state, while those events
describe state transitions.

## Bounded performance timeline

Any schema 1 snapshot can be normalized into Chrome Trace Event JSON for Chrome
DevTools, Perfetto or another compatible viewer:

```bash
pam diagnostics > snapshot.json
pam timeline snapshot.json --output timeline.json

# The bounded snapshot can also stay entirely in a pipe:
pam diagnostics | pam timeline - > timeline.json
```

`--output` uses create-new semantics and never overwrites existing evidence.
Input is limited to a regular, non-symlink file of at most 1 MiB, or the same
bounded amount from standard input. Server snapshots accept at most 1,024
events; Native accepts its protocol limit of eight; Desktop exports its bounded
aggregate counters as a Trace Event counter sample.

Server monotonic nanoseconds are rebased to the first event. Native durations
are placed sequentially because schema 1 deliberately does not persist device
timestamps. Desktop schema 1 has aggregate metrics rather than individual
events. These distinctions remain explicit: the exporter does not fabricate
wall-clock precision or individual Desktop command spans.

Only static PAM event names, integer timings, failure state and operational
counters enter the trace. Server `context` and request IDs, Native labels, and
unknown Desktop fields are discarded. This makes exported evidence useful for
performance comparison without turning a diagnostic artifact into an
application-data or credential archive.
