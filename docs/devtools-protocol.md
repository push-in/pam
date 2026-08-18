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
