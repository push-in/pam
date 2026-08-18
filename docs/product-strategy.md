# PAM product strategy

This document turns PAM's broad product ambition into an evidence-driven program.
It is a planning artifact, not a claim that every listed capability already
exists. Items are explicitly classified as **shipped**, **prove**, or **explore**.

## Vision

PAM should be the coherent PHP application platform: one language, package
ecosystem, CLI, persistent runtime, and operational model spanning servers,
native mobile applications, and secure desktop software.

PAM wins when a PHP team can build an excellent product on any supported target
without accepting a toy runtime, an opaque native boundary, or a fragmented
release workflow.

## Product principles

1. **PHP remains the product language.** Rust, Kotlin, Swift, and platform code
   form a deliberate systems boundary rather than leaking into normal app work.
2. **Native means platform-native.** PAM Native uses native controls; PAM
   Desktop grants native powers explicitly. Marketing must name WebViews or
   embedded engines wherever they are used.
3. **Warm does not mean shared accidentally.** Persistent state, request
   isolation, cancellation, cleanup, and bounded queues are one correctness
   contract.
4. **One command should reveal the next action.** Diagnostics must explain the
   cause, affected target, safe remediation, and verification command.
5. **Performance claims require reproducible evidence.** Compare the same app,
   host, worker count, load shape, warmup, and correctness gates.
6. **Generated state is disposable and bounded.** Development caches and build
   artifacts may improve iteration speed but must not grow without control.
7. **Security and accessibility are product features.** They are release gates,
   not optional documentation sections.
8. **Compatibility is executable.** Supported PHP, Laravel, OS, device, and
   plugin versions need fixtures and CI evidence.

## Competitive map

The comparison uses official project documentation, checked on 2026-08-18.
Scores are intentionally omitted until PAM has a repeatable scoring harness.

| Surface | Reference | Demonstrated strength | PAM response |
| --- | --- | --- | --- |
| Server | [FrankenPHP](https://frankenphp.dev/docs/) | Caddy integration, worker mode, automatic HTTPS, HTTP/2 and HTTP/3, hot reload, broad PHP compatibility, and standalone binaries | **Prove:** publish like-for-like compatibility, latency, throughput, memory, reload, and failure-recovery evidence. **Explore:** a similarly direct production bootstrap without hiding PAM's isolation model. |
| Server | [RoadRunner](https://docs.roadrunner.dev/docs) | Extensible worker/process manager with HTTP, jobs, gRPC, TCP, Temporal, and Centrifugo integrations | **Shipped:** supervised workers and native transports exist. **Explore:** a stable plugin contract for queues and non-HTTP services with one observability model. |
| Mobile PHP | [NativePHP](https://nativephp.com/docs/) | Clear PHP/Laravel positioning across mobile and desktop | **Shipped:** PAM Native renders Android and UIKit controls without a JavaScript runtime. **Prove:** complete device matrix, public app artifacts, startup/FPS/memory budgets, and plugin compatibility. |
| Mobile | [React Native](https://reactnative.dev/architecture/landing-page) | Production-scale ecosystem, concurrent renderer, typed native modules/components, and direct JSI interop | **Prove:** renderer correctness, list/input/navigation performance, accessible components, and debugging quality. **Explore:** generated typed plugin bindings and priority-aware rendering. |
| Mobile | [Flutter](https://docs.flutter.dev/tools/hot-reload) | Fast, state-preserving hot reload and a coherent UI/tooling system | **Shipped:** PAM has state-preserving native-tree reload paths. **Prove:** publish reload latency and preservation behavior by edit category. **Explore:** visual component preview and golden tests. |
| Desktop | [Tauri](https://tauri.app/concept/architecture/) | Small Rust-based applications, system WebViews, composable plugins, and explicit JS/Rust invocation | **Shipped:** PAM Desktop uses a Rust boundary and explicit capabilities. **Prove:** package size, memory, startup, permission enforcement, signing, and updates on all desktop targets. |
| Desktop | [Electron](https://www.electronjs.org/docs/latest/tutorial/security) | Mature Chromium compatibility, process model, packaging, updates, and a large ecosystem | **Shipped:** capability-scoped PAM commands reduce ambient renderer privilege. **Explore:** hardened renderer isolation, permission auditing, crash recovery, and update ergonomics while keeping the smaller trusted boundary. |

### Competitive research refresh — 2026-08-18

Official documentation shows observability becoming part of the product rather
than an optional integration. [FrankenPHP exposes Prometheus worker/thread,
queue, latency, crash and restart metrics](https://frankenphp.dev/docs/metrics/)
and presents a zero-configuration TUI/exporter in its
[observability guide](https://frankenphp.dev/docs/observability/).
[RoadRunner](https://docs.roadrunner.dev/docs/logging-and-observability/otel)
connects HTTP, jobs, gRPC and other plugins through OpenTelemetry, although its
own documentation still labels only tracing stable for production.

The same convergence exists in application tooling. React Native DevTools now
places JavaScript execution, React work, network activity and user timings in a
single [performance timeline](https://reactnative.dev/docs/react-native-devtools),
while Tauri generates schemas for explicit per-window capabilities and warns
that overlapping capabilities merge security boundaries in its
[capability model](https://v2.tauri.app/security/capabilities/).

PAM already has bounded Prometheus metrics, a live `top`, structured logs,
cross-surface diagnostics and capability audits. The immediate correctness gap
was distributed trace lineage: an accepted W3C `traceparent` was previously
echoed with its caller parent ID. PAM now retains the incoming trace ID and
flags but creates a distinct server span ID, allowing logs and downstream
requests to form a real parent-child tree. The next differentiator is a bounded
cross-surface performance timeline and direct OTLP/HTTP JSON export. Both are
now shipped with bounded data paths and reproducible official-Collector
certification. Native now maps each family to its real OTLP signal endpoint;
the next step is deliberately opt-in Desktop spans rather than another
isolated debug screen.

## Current product audit

### PAM runtime and CLI

**Shipped in this repository**

- Embedded persistent PHP runtime with supervised workers.
- HTTP/1.1, HTTP/2, HTTP/3, WebSockets, streaming, health, metrics, diagnostics,
  profiling, tracing, reload, and watchdog behavior.
- Laravel and Octane integration with executable integration tests.
- Project discovery, guided initialization, doctor/repair, generators, ecosystem
  capabilities, builds, packages, release checks, shell completion, and editor
  support.
- Reproducible benchmark and soak protocols.

**Needs stronger evidence**

- Public clean-host installation and upgrade journeys.
- Competitive benchmark artifacts generated from immutable releases.
- Long-duration memory, cancellation, and worker-recovery results.
- Windows behavior and common container/orchestrator deployment matrices.
- A structured, stable diagnostics schema across all targets.

### PAM Native

**Shipped in the checked-out native repository**

- Rust reconciliation/layout boundary and Android/iOS native hosts.
- Typed components, navigation, animations, gestures, lists, input handling,
  state restoration, hot reload, native modules/views, and plugin metadata.
- Official ecosystem packages for common device and product capabilities.
- Android profiling/benchmark paths and generated iOS host contracts.
- Project-scoped development artifacts with automatic cleanup at dev startup.
- Deterministic Android/iOS release-authority audit with human and stable JSON
  output, sequential integer severities, and configurable CI denial threshold.

**Needs stronger evidence**

- Real-device Android and iOS release certification in CI.
- Published frame-time, startup, memory, binary-size, and reload-latency budgets.
- Screenshot/golden tests and accessibility conformance evidence.
- A public compatibility registry for plugins and PAM runtime releases.
- A cohesive, production-grade design system and visual component workbench.

### PAM Desktop

**Shipped by contract/integration**

- Contextual CLI delegation to the separately distributed `pam-desktop` binary.
- Servo-based local UI, registered commands/events, explicit capabilities,
  multiple windows, packaging, signed update metadata, and rollback are described
  by current project documentation.

**Needs stronger evidence**

- The desktop source and release artifact must be audited as a separate product;
  delegation from this repository does not prove implementation quality.
- Cross-platform CI artifacts, signing/notarization, sandboxing, accessibility,
  Web compatibility, crash reporting, startup/memory/CPU budgets, and real
  graphical-session evidence. Linux package footprint and update recovery have
  deterministic gates.
- A documented fallback policy where Servo does not meet application needs.

## North-star outcomes and metrics

Baselines must be captured before targets are frozen. A metric is not green
without a reproducible command, environment metadata, raw artifact, and failure
threshold.

| Outcome | Required metrics |
| --- | --- |
| First success | Clean-machine install success; time to `pam dev`; time to first server response or rendered frame; number of manual prerequisites |
| Fast iteration | Warm reload p50/p95; state-preservation rate by edit category; native rebuild frequency; bytes regenerated; cache growth per hour |
| Runtime quality | Throughput, p50/p95/p99 latency, RSS, PHP memory, event-loop lag, error rate, recovery time, and 8/24-hour soak slope |
| Native quality | Cold/warm startup, JS-free/PHP-runtime startup cost, UI and JS/PHP thread frame time, dropped frames, list throughput, input latency, memory, and package size |
| Desktop quality | Cold/warm startup, idle/active RSS and CPU, installer size, update/rollback success, renderer crash recovery, and capability-denial coverage |
| Reliability | Supported-version CI pass rate, flaky-test rate, crash-free sessions, successful upgrades, and mean diagnosis time |
| Ecosystem | Verified plugins, compatibility coverage, install success, time to create a plugin, and percentage of APIs with runnable examples |
| Inclusion/security | Automated accessibility coverage, manual screen-reader journeys, reduced-motion behavior, fuzz duration, dependency freshness, and release security gates |

## Prioritized roadmap

### P0 — Evidence and developer trust

1. Version this strategy and keep the feature audit honest.
2. Introduce a machine-readable benchmark/evidence manifest shared by Server,
   Native, and Desktop.
3. Make `pam info`, `pam doctor`, and development startup expose the active
   target, artifact footprint, relevant paths, and exact next actions.
4. Keep every development workspace project-scoped and bounded.
5. Publish clean-host install, benchmark, soak, and package artifacts from CI.

### P1 — PAM Dev Experience 2.0

1. Unified development session header and lifecycle events.
2. Structured `--json` output for automation alongside excellent human output.
3. Error envelopes containing code, cause, remediation, and verification.
4. One DevTools protocol for runtime, native tree, desktop bridge, logs, network,
   performance, and state inspection.
5. Component preview, screenshot tests, and app-level test driver.

### P2 — Product differentiation

1. Server plugin/capability contract for jobs and non-HTTP transports.
2. PAM Native design system with accessible adaptive components and tokens.
3. Typed plugin registry with signed metadata and executable compatibility.
4. PAM Desktop hardening, updater recovery, permission audit, and platform CI.
5. A flagship application using PAM Server, Native, Desktop, and shared packages.

### P3 — Ecosystem scale

1. Stable extension APIs with deprecation windows and migration tooling.
2. Public compatibility and performance dashboards.
3. Curated templates for real product categories.
4. Contributor certification fixtures and plugin conformance suites.

## Decision gates

A roadmap item enters implementation only when it has:

- a user problem and target surface;
- authoritative current-state evidence;
- a measurable acceptance criterion;
- a compatibility and migration impact;
- a security, accessibility, and operational review where applicable;
- a test or artifact that proves completion.

An item is complete only when the evidence is reproducible from a clean checkout.
Screenshots, benchmarks, and release claims must identify the exact commit and
environment that produced them.

## Execution ledger

This ledger separates shipped work from open product claims. Commit hashes are
repository-local.

| Roadmap evidence | Runtime / CLI | Native | Desktop | State |
| --- | --- | --- | --- | --- |
| Product audit, competitor map, metrics, roadmap | `a363bb7` | Covered by the cross-product audit | Covered by the cross-product audit | Shipped |
| Project-scoped bounded development artifacts | Cleanup contract `c7c79c7`; measured 9.51 GB reclaimed across the three local product workspaces | Android/iOS generated hosts and Cargo targets use the same scoped command | Host cache retention `f895be8`; Cargo target uses the shared cleanup contract | Shipped |
| Verifiable benchmark evidence | `58ca0e0`, workflow `1fcd2cd` | Manifest `742e0e4` | Attested package reproducibility `acc3a7c`; authenticated footprint and 5% release-baseline gate `2dad206` | Shipped; public runs depend on CI execution |
| Structured error and automation contracts | `fc1a8fa`; actionable Doctor target/artifact/remediation report `fe18c67` | Contextual CLI commands inherit the envelope | Desktop retains its typed bridge errors | Shipped |
| Development lifecycle event protocol | `c273d21` | Android/iOS hosts emit schema 1 | `392a6eb` | Shipped |
| Versioned DevTools snapshots | `1b3f3f3` | `d4dec09` | `ea34d52` | Shipped |
| Cross-surface observability | Prometheus/control-plane metrics, structured access logs and W3C server-child trace lineage `338c780`; redacted Chrome/Perfetto timeline exporter `7d334f0`; bounded OTLP/HTTP JSON server spans `5884a71`; signed official-Collector certification and evidence `e5811cf` | Bounded offline events plus certified signal-correct OTLP traces, logs, delta counters, and gauges in the optional observability package `ab4805e` | Command aggregates normalize as a bounded counter event without bridge data; production export remains off | Server and Native Collector certification shipped; explicit Desktop span capture remains open |
| Contextual live snapshot transport | Desktop routing `77f578f`; Android routing `bddab4c`; iOS routing `15c32cb` | Privilege-gated Android export `5b4b5f5`; app-scoped iOS Simulator export and generated overlay `8a95f55` | Authenticated development session `47b489b` | Server, Android, iOS Simulator and Desktop shipped; physical-device Native export intentionally excluded pending a pairing protocol |
| Visual capture foundation | `20fb5cc` | Scoped Android/iOS PNG capture | Pixel-normalized golden harness `361287d` | Shipped; platform capture remains user-mediated |
| Accessible adaptive design tokens | Desktop starter run-green identity and WCAG/forced-color gate `0634ce7` | Native tokens `b07d09f`; contrast-gated PAM Mobile UI themes and Studio `9099df7` | Generated Desktop starter inherits the runtime-owned design contract | Native system and Desktop first-run surface shipped; reusable cross-surface package remains open |
| Cross-surface release authority and recovery | iOS audit artifact workflow `9018383` | Native release audit `d4746b9` | Permission policy `46fead6`; interrupted updater recovery `e82efd6` | Native/Desktop policy shipped; platform sandbox certification remains open |
| Signed typed plugin registry | Offline schema 1 verifier, quorum rotation, rollback floor and SemVer/protocol resolver `6238dfc`; authenticated `pam add` gate `66b18aa`; exact verified-byte Composer source and bounded artifact retention `d49713b`; recoverable project rotation adoption `a36a482`; canonical ceremony payload, Ed25519 key identity and operational runbook `8b14058` | Descriptor and IDL integrity locks; signed Android runtime installer and provenance `b1e7dbb` | Executable protocol/identity/hash checks; signed host acquisition, provenance and bounded retention | Composer, Android runtime and Desktop host installer enforcement implemented; official multi-custodian ceremony, independent fingerprint publication and hosted catalog remain open |
| Flagship cross-surface application | — | Native showcase exists | — | Open |

The next registry gate is intentionally operational: conduct the independent
production-key ceremony, publish the root hash through a PAM release and a
second channel, and sign the initial catalog. Composer, Android Native and the
Desktop host now enforce resolver output and persist the accepted sequence;
iOS runtime delivery remains source-integrated rather than a standalone
downloaded binary artifact.
The canonical payload, quorum rotation, revocation model and deterministic
offline tamper fixtures now exist in `6238dfc`; a locally generated private key
or unsigned hosted JSON still does not satisfy the public-registry claim.
