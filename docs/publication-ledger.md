# Delivery publication ledger

This ledger distinguishes source publication from a versioned product release.
Pushing `main` makes reviewed source and CI workflows public; it does not create
a tag, Composer release, signed binary, marketplace entry or support promise.
Those artifacts continue to require the repository's release/version gates.

## 2026-08-18 platform batch

| Repository | Source range prepared for `main` | Scope |
| --- | --- | --- |
| [`push-in/pam`](https://github.com/push-in/pam) | `a6bc489..4bbb14a`, plus this ledger/reference repair | Runtime/CLI evidence foundations, diagnostics, release audits, signed registry enforcement, cross-surface timelines and certified Server OTLP |
| [`push-in/pam-desktop`](https://github.com/push-in/pam-desktop) | `0a498a9..e2c7840` | updater/permission hardening, diagnostics, package evidence, authenticated host acquisition, certified OTLP command spans and scoped outbound context |
| [`push-in/pam-native-observability`](https://github.com/push-in/pam-native-observability) | `615e64a..622df52` | signal-correct OTLP traces/logs/metrics, official Collector certification and strict Server-to-Native W3C lineage |

The PAM history was rebased over remote governance gate `a6bc489`. Documentation
commit references were remapped by exact commit subject and checked so the
published evidence points to the rewritten source history.

## Verification recorded before publication

- PAM Runtime OTLP payloads and parent lineage were accepted by the immutable
  official OpenTelemetry Collector `0.157.0` image.
- PAM Desktop passed its gateway workspace tests, strict Clippy/rustfmt gates,
  real Collector acceptance, trace-parent lineage inspection and real outbound
  header capture.
- PAM Native Observability passed all functional tests, PHPStan level 9 and
  official Collector acceptance for traces, logs, counters, gauges and remote
  parent lineage.
- Collector containers/images, Composer `vendor` directories and temporary
  Cargo targets created by these checks were removed afterward. No generated
  build cache is committed or retained inside these repositories.

## Publication procedure

1. Fetch each `origin/main` and require zero commits behind before pushing.
2. Push the exact local `main` without force.
3. Record the remote commit SHA returned by GitHub.
4. Inspect every workflow triggered by that SHA and do not call the batch
   published-green while a required check is pending or failing.
5. Correct failures with ordinary follow-up commits; never rewrite published
   `main` to hide delivery history.

Local nested repositories and the pre-existing Native macrobenchmark change are
outside this batch. They are not staged, committed or implicitly published by
the PAM repository push.
