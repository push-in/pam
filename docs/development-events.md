# Development session events

PAM exposes one opt-in, versioned event stream across server, Android, iOS, and
desktop development sessions. It is intended for DevTools, editor extensions,
CI harnesses, and observability collectors while preserving the normal human
terminal output.

Enable it for any development command:

```bash
PAM_DEV_EVENTS=1 pam dev public/index.php
PAM_DEV_EVENTS=1 pam mobile dev
PAM_DEV_EVENTS=1 pam mobile ios:dev
PAM_DEV_EVENTS=1 pam-desktop dev
```

`json` and `jsonl` are accepted as aliases for `1`. Events are written to
standard error as single-line JSON prefixed by `@pam-event `. Consumers can
therefore extract protocol records even when application logs share the same
stream. When the variable is absent, PAM emits no protocol records.

## Envelope schema 1

```json
{
  "schemaVersion": 1,
  "eventCode": 5,
  "surfaceCode": 2,
  "sessionId": "4128-1787068123456",
  "sequence": 4,
  "occurredAtUnixMs": 1787068123678,
  "projectRoot": "/workspace/example",
  "data": { "bundleVersion": "a1b2c3" }
}
```

All discriminator values are sequential integers. Consumers must ignore
unknown fields, unknown event codes, and unknown surface codes so schema 1 can
gain additive data safely.

### Event codes

| Code | Enum | Meaning |
| ---: | --- | --- |
| 1 | `SessionStarting` | Host startup began. |
| 2 | `SessionReady` | The development host can serve the application. |
| 3 | `ChangeDetected` | A watched project input changed. |
| 4 | `ReloadStarted` | Rebuild or runtime replacement began. |
| 5 | `ReloadSucceeded` | The new application state is ready. |
| 6 | `ReloadFailed` | Reload failed; `data.message` contains the diagnostic. |
| 7 | `RuntimeExited` | A supervised application process exited. |
| 8 | `SessionStopped` | The host stopped normally. |

### Surface codes

| Code | Enum |
| ---: | --- |
| 1 | `Server` |
| 2 | `Android` |
| 3 | `Ios` |
| 4 | `Desktop` |

`sequence` is monotonic within the emitting process. `sessionId` is opaque and
stable for that process. Timestamps use Unix epoch milliseconds. Fields under
`data` are event-specific; reload variants use integer `reloadCode` values
(`1` for assets and `2` for runtime) rather than string discriminators.

## Compatibility policy

- Existing event codes and meanings do not change within schema 1.
- New optional `data` fields may be added without a schema bump.
- Removing or changing a field, code, or unit requires schema 2.
- Human terminal copy, colors, and application logs are outside this protocol.
