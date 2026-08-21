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

The comparison uses official project documentation, checked on 2026-08-19.
Scores are intentionally omitted until PAM has a repeatable scoring harness.

| Surface | Reference | Demonstrated strength | PAM response |
| --- | --- | --- | --- |
| Server | [FrankenPHP](https://frankenphp.dev/docs/) | Caddy integration, worker mode, automatic HTTPS, HTTP/2 and HTTP/3, hot reload, broad PHP compatibility, and PHP 8.5 standalone distribution for Linux, macOS and Windows | **Prove:** publish like-for-like compatibility, latency, throughput, memory, reload, failure recovery, and clean-host installation evidence. **Build next:** a similarly direct verified bootstrap without hiding PAM's system-ABI or isolation model. |
| Server | [RoadRunner](https://docs.roadrunner.dev/docs) | Extensible worker/process manager with HTTP, jobs, gRPC, TCP, Temporal, and Centrifugo integrations | **Shipped:** supervised workers, native transports, persisted desired state, automatic master recovery, bounded exponential backoff, and a resettable circuit breaker. **Explore:** a stable plugin contract for queues and non-HTTP services with one observability model. |
| Process manager | [PM2 restart strategies](https://pm2.keymetrics.io/docs/usage/restart-strategies/), [environment variables](https://pm2.keymetrics.io/docs/usage/environment/) and [application declaration](https://pm2.keymetrics.io/docs/usage/application-declaration/) | Automatic crash restart, configurable delay, capped exponential backoff, stable-uptime reset, restart limits, detached logs, named environments, and declarative applications | **Shipped:** PAM persists explicit online/stopped intent, bounded recovery/backoff/circuit state, private environment references and integer-coded evidence. Linux `pidfd` and deadline-aware scheduling first reduced hosted PAM master/worker p95 from 668 to 201 ms. The [event-driven hosted comparison](https://github.com/push-in/pam/actions/runs/32504298849) unifies pidfd and command-socket wakeups in one allocation-stable poll set, publishes detection/backoff/readiness timestamps and phase gates, and recovered 10/10 with PAM p50/p95 167/169 ms versus PM2 direct-process 138/146 ms. All gates and the independently verified nine-artifact bundle passed; RSS remains explicitly non-comparable. The [hosted suite-7 matrix](https://github.com/push-in/pam/actions/runs/32507621605) recovered 30/30 on clean Linux: total p95 was 169/241/580 ms and readiness p95 92/165/485 ms for 1/4/16 workers; all gates and the independently verified 17-artifact bundle passed. The [diagnostic follow-up](https://github.com/push-in/pam/actions/runs/32510174077) also recovered 30/30 and independently verified 20 artifacts. At 16 workers, spawn-spread p95 was only 36 ms against 479 ms spawn-to-ready p95, locating the dominant slope in concurrent PHP/application bootstrap rather than serial process launch; the paired total/readiness p95 changes (+35/+22 ms) do not support claiming a polling improvement. **Build next:** reduce measured bootstrap work or prove a safe reuse/preload design while preserving complete-generation readiness and master/worker isolation. |
| Operations | [Kubernetes liveness, readiness and startup probes](https://kubernetes.io/docs/concepts/workloads/pods/probes/) | Distinguishes startup/readiness from ongoing liveness, suppresses liveness until startup succeeds, and restarts containers after repeated failed liveness checks | **Shipped:** PAM complements generation readiness with bounded asynchronous HTTP liveness probes, a per-master bounded startup period, consecutive-failure thresholds, stale-result rejection and the existing backoff/circuit breaker. Loopback-only targets avoid turning the per-user daemon into an SSRF primitive. **Prove next:** benchmark detection-to-recovery latency for blocked workers under load. |
| Server | [OpenSwoole](https://openswoole.com/docs/protocols) | Coroutine-first PHP extension with HTTP/2, WebSocket, TCP/UDP, MQTT, gRPC and coroutine clients; explicit backpressure and coroutine limits | **Shipped:** PAM has HTTP/1.1–3, WebSockets, native transports, bounded queues and worker supervision. **Prove:** blocking-I/O detection, saturation behavior and protocol comparisons. **Explore:** a stable async service/plugin contract without weakening request isolation. |
| Server ecosystem | [Laravel Octane](https://laravel.com/docs/12.x/octane) | First-party Laravel workflow across FrankenPHP, RoadRunner and Swoole, including reload, worker lifecycle guidance and production supervision | **Shipped:** PAM implements the Octane server contract and lifecycle commands. **Prove:** framework compatibility against all supported Laravel/PHP pairs and publish migration evidence from each documented Octane server. |
| Mobile PHP | [NativePHP Mobile v4](https://nativephp.com/docs/mobile/4/architecture/about-the-new-architecture) | Laravel on-device, native SwiftUI/Jetpack Compose rendering without a WebView, shared-memory interaction, and Composer plugins carrying PHP, Swift, Kotlin, UI components, native dependencies and lifecycle hooks | **Corrected position:** native rendering without JavaScript is now category parity, not a PAM-only differentiator. **Prove:** open fail-closed contracts, plugin provenance/compatibility, device certification, startup/frame/memory budgets and reproducible store artifacts. Treat plugin build hooks and repositories as supply-chain authority, not convenience metadata. |
| Mobile | [React Native](https://reactnative.dev/architecture/landing-page) | Production-scale ecosystem, concurrent renderer, typed native modules/components, and direct JSI interop | **Prove:** renderer correctness, list/input/navigation performance, accessible components, and debugging quality. **Explore:** generated typed plugin bindings and priority-aware rendering. |
| Mobile | [Flutter](https://docs.flutter.dev/tools/hot-reload) | Fast, state-preserving hot reload, coherent UI/tooling, profile-mode performance inspection, and release-size analysis/diff artifacts | **Shipped:** PAM has state-preserving native-tree reload paths. **Prove:** publish reload latency and preservation behavior by edit category plus store-representative download/install size. **Explore:** visual component preview and per-package size attribution. |
| Desktop | [Tauri](https://tauri.app/concept/architecture/) | Small Rust-based applications, system WebViews, composable plugins, explicit JS/Rust invocation, and updater signatures that cannot be disabled | **Shipped:** PAM Desktop uses a Rust boundary, explicit capabilities, signed metadata and rollback. **Prove next:** signed installer/update artifacts, key-loss recovery policy, clean-host startup, package size and permission enforcement on every desktop target. |
| Desktop | [Electron](https://www.electronjs.org/docs/latest/tutorial/security) | Mature Chromium compatibility, process model, packaging, updates, and a large ecosystem | **Shipped:** capability-scoped PAM commands reduce ambient renderer privilege. **Explore:** hardened renderer isolation, permission auditing, crash recovery, and update ergonomics while keeping the smaller trusted boundary. |
| Desktop engine | [Servo](https://servo.org/blog/2026/07/31/june-in-servo/) | A lightweight embeddable engine with an evolving embedding API and work toward a stable C ABI | **Shipped:** PAM pins the Rust embedding API and compiles the real host on Linux x64, macOS arm64 and Windows x64. **Prove next:** signed installers, clean-machine launch, accessibility coverage and renderer recovery before broadening support claims. |

### Competitive research refresh — 2026-08-21 progressive delivery

The official [Kubernetes Deployment contract](https://kubernetes.io/docs/concepts/workloads/controllers/deployment/)
separates readiness, availability, progress deadlines, bounded revision history,
pause/resume and rollback. It also makes the capacity tradeoff explicit through
`maxUnavailable` and `maxSurge`; a rollout may temporarily consume more than the
steady-state replica target. PAM must therefore report candidate and stable
capacity independently and may not call a process ready merely because it has a
PID.

The official [Gateway API traffic-splitting guide](https://gateway-api.sigs.k8s.io/guides/user-guides/traffic-splitting/)
defines backend weights as relative values and demonstrates both a 90/10 canary
and header-selected preview traffic. PAM will use integer basis points for its
public contract, deterministic affinity for a stable request key, and an
explicit preview override that cannot be supplied through an untrusted generic
forwarding header. Weight changes must be atomic, observable and reversible.

This raises the acceptance bar beyond the already shipped transactional
stop/start deploy:

1. start the candidate beside the stable release on an isolated loopback
   listener and pass readiness before exposing traffic;
2. retain the stable release while weight is below 10,000 basis points;
3. expose per-version request, error and latency evidence with bounded label
   cardinality;
4. abort by atomically restoring stable weight to 10,000 and draining the
   candidate; promote by moving candidate to stable only after an explicit gate;
5. persist rollout phase and history with sequential integer enums, deadlines
   and crash recovery;
6. account for temporary surge in worker, memory and task limits.

The existing pool ingress is not evidence for this feature: it selects
specialized worker groups from one release and requires one fallback. A separate
version-ingress authority is required so PAM cannot market capacity routing as
blue-green delivery.

**Shipped on Linux:** the separate version ingress now provides atomic weighted
stable/candidate routing, deterministic affinity, streaming HTTP and WebSocket
proxying, per-version evidence, explicit abort/promote, and certificate-backed
TLS termination. Its TLS contract requires bounded regular non-symlink PEM files,
does not disclose key paths through status, and preserves HTTP/2 externally while
normalizing the explicit HTTP upstream hop. Generation-scoped metrics, persisted
phases/deadlines and automatic minimum-sample/error/deadline gates are also
shipped. Remaining acceptance work is candidate process lifecycle orchestration,
trusted preview selection, connection draining and surge accounting.

For Linux resource governance, systemd's official
[`pam_systemd` documentation](https://www.freedesktop.org/software/systemd/man/pam_systemd.html)
maps session limits to `MemoryMax=`, `TasksMax=` and CPU weight controls, while
also warning that the per-user service manager sits outside an individual login
session scope. PAM's systemd user unit alone therefore does not prove per-app
isolation. The target is one transient scope/service per managed application (or
a delegated cgroup-v2 subtree), with requested and observed limits shown in
`describe`, and fail-closed behavior when a mandatory limit cannot be applied.

**Shipped observation baseline on Linux:** `status`, `describe` and `monit` now
aggregate RSS, threads and process count across each supervisor's descendant
tree, compare them with declarative positive warning thresholds, and expose
stable sequential alert codes. Threshold-only reconciliation is live and does
not restart a healthy process. This establishes observed evidence; hard
MemoryMax/TasksMax/CPU enforcement and cgroup event counters remain required.

**Shipped private dashboard baseline on Linux:** `dashboard` turns the same live
manager records into a bounded, owner-only, dependency-free HTML flight recorder.
It exposes textual process and capacity signals without commands, paths,
environment values, network data, or logs; it never overwrites prior evidence.
Remote fleet aggregation remains future work and must preserve this local-first
privacy boundary.

**Shipped bounded history baseline on Linux:** `pamd` now samples each managed
application once per minute into independent owner-only 120-entry records.
`monit:history` exposes exact versioned evidence and explicit incident capture;
the static dashboard adds peak and textual trend summaries. Records exclude
commands, paths, environment, network data, and logs, and application deletion
removes its history. Longer retention, downsampling, and fleet aggregation remain
future opt-in layers rather than unbounded defaults.

**Shipped live local dashboard baseline on Linux:** `dashboard:start/status/stop`
provides a detached, read-only, loopback-only HTTP view with mandatory private
file credentials, Basic/Bearer constant-time verification, bounded requests,
no-store/CSP hardening and explicit accessible refresh. PAM persists only the
credential digest and removes it on stop. This closes the local web-operations
gap without requiring the cloud account and agent linkage documented by
[PM2 Plus](https://pm2.keymetrics.io/docs/plus/quick-start/); cross-host access
remains a separate future design requiring TLS and stronger fleet identity.

**Shipped enforcement baseline on Linux:** applications that opt into
`memory_max_bytes` and/or `task_max_count` launch fail-closed in unique transient
systemd user scopes. PAM reads the process's actual cgroup-v2 `memory.max` and
`pids.max`, distinguishes verified/not-requested/unverified with sequential
integer codes, and restarts only when a hard limit changes. CPU quotas/weights,
OOM/event counters and non-systemd delegated cgroups remain open.

### Competitive research refresh — 2026-08-20 Windows Runtime architecture

PHP's official [Embed SAPI documentation](https://github.com/php/php-src/blob/master/sapi/embed/README.md)
states that Embed is disabled by default and PHP must be rebuilt with it enabled.
The [official Windows distribution](https://windows.php.net/) documents CLI,
FastCGI and Apache-oriented TS/NTS binaries, but does not establish those ZIPs as
an Embed SDK. The official
[`php-windows-builder`](https://github.com/php/php-windows-builder) supports
reproducible x64 builds on Visual Studio 2022 for PHP 8.4 and 8.5.

PAM will therefore not claim Windows Runtime support by wrapping the public CLI
ZIP or by substituting a per-request subprocess for its persistent Embed model.
The Windows gate requires a pinned `php-src` revision and pinned Windows SDK
toolchain, an explicitly enabled Embed SAPI, a complete DLL/header/import-library
inventory, Rust linkage through MSVC, and a relocatable `pam-run.exe` package.
Certification must run on a clean `windows-2022` host and bind first PHP success,
loaded modules without startup warnings, installed footprint, GitHub attestation,
upgrade and rollback to the greatest older stable release. Until that producer
and hosted evidence exist, Windows remains an explicit unsupported Runtime
target rather than inferred cross-platform parity.

The first source boundary is now implemented: PAM's build script resolves the
Cargo target instead of the build-script host, accepts only `php*embed.lib` for
Windows, consumes explicit bounded SDK include roots, invokes MSVC `cl.exe` and
`lib.exe`, and emits no ELF/Mach-O rpath flags for that target. Executable tests
prove a generic `php8.lib` or DLL cannot be mistaken for the Embed import
library. PHP argument conversion is target-scoped instead of importing Unix
`OsStrExt` unconditionally. This is build-foundation evidence only: remaining
Unix-only process supervision, secure-file handling and packaging must be ported
before the hosted Windows producer is enabled or support is claimed.

The admin-token file boundary is also target-aware now. Windows opens the path
with `FILE_FLAG_OPEN_REPARSE_POINT`, rejects non-regular/reparse inputs, and
compares the opened handle's volume serial plus file index with the inspected
metadata before reading the same 258-byte maximum used on Unix. A local
Windows-target check reached the native AWS-LC build and correctly stopped
without MSVC/NASM on the Linux host; this is not Windows compilation evidence.
The hosted job must compile every target-specific module with the real Visual
Studio toolchain before this source boundary can move to shipped.

### Competitive research refresh — 2026-08-20 Desktop installer trust

The current Desktop distribution baseline is explicit in official tooling.
[Tauri's macOS signing guide](https://v2.tauri.app/distribute/sign/macos/)
requires platform signing for browser distribution and Developer ID
notarization, while its
[Windows signing guide](https://v2.tauri.app/distribute/sign/windows/)
ties trusted browser distribution and Microsoft Store delivery to Windows code
signing. [Electron's official signing guide](https://www.electronjs.org/docs/latest/tutorial/code-signing)
likewise separates packaging from OS trust and requires signing plus Apple
notarization for macOS distribution. PAM therefore no longer treats the generic
distribution signature check as sufficient native-installer proof: schema 1
Desktop native-installer evidence must bind the exact installer digest to a typed platform report
covering publisher verification, notarization or explicit non-applicability,
sandbox enforcement and interrupted-update recovery. This closes the portable
verification contract; native signed-installer producers and their first hosted
clean-machine runs remain open and are not implied by this change.

### Competitive research refresh — 2026-08-20 updater authority

[Tauri's official updater contract](https://v2.tauri.app/plugin/updater/)
requires signatures and places the version, target URL and signature together in
the update feed; its default comparator accepts only a version newer than the
installed application. [Electron's official autoUpdater API](https://www.electronjs.org/docs/latest/api/auto-updater/)
likewise defaults Windows MSIX away from arbitrary-version installation and
requires an explicit `allowAnyVersion` opt-out, while macOS automatic updates
require application signing. The
[TUF specification](https://theupdateframework.github.io/specification/latest/)
treats rollback/freeze resistance and trusted-root rotation as metadata duties,
not properties supplied by TLS alone.

PAM now matches the relevant local guarantees with a stricter explicit recovery
path: canonical signed `releaseVersion`, exact target and candidate digest are
one Ed25519 authority; normal check/install requires a greater SemVer, downgrade
requires a named version plus `--allow-downgrade`, and a two-key compiled bridge
supports pre-announced rotation. Automatically discovered updates additionally
require a paired signed issue/expiry window of at most 31 days, including when
the discovery response claims the installed version is current. Replayed
discovery therefore fails closed after a bounded interval, while an explicitly
named historical version remains available for audited recovery. This is an
inference from the cited contracts, not a claim of full TUF equivalence. PAM
still uses each target manifest as both targets and freshness metadata instead
of separating timestamp/snapshot/targets roles, and its signing authority is not
an offline threshold root. Suppression can still deny updates, but it can no
longer make stale automatic discovery appear fresh beyond the signed deadline.

### Competitive research refresh — 2026-08-19 distribution and trust

The distribution baseline has moved beyond “a release binary exists.”
[FrankenPHP's current installation documentation](https://frankenphp.dev/docs/)
advertises PHP 8.5 standalone delivery on Linux, macOS, and Windows and calls the
Linux artifacts statically linked. PAM currently documents a compatible-system
ABI requirement instead. That distinction must remain explicit until a
clean-host matrix proves every runtime dependency, first response, upgrade, and
rollback from immutable release assets.

Desktop update trust is equally concrete. The
[Tauri updater contract](https://v2.tauri.app/plugin/updater/) requires a
signature and does not permit disabling verification; it also produces distinct
AppImage, macOS archive, MSI, and NSIS update artifacts. PAM Desktop already
verifies signed metadata and interrupted-update recovery, but source CI is not
a substitute for signed installers launched on clean target systems. Key-loss,
rotation, rollback floor, and offline recovery must be part of the same
certification rather than operational prose added after release.

On mobile, [Flutter's app-size workflow](https://docs.flutter.dev/perf/app-size)
separates debug/upload size from user download/install size, supports release
size analysis, and allows two analysis artifacts to be diffed. PAM's binary-size
gate must likewise identify architecture, density/thinning, compressed download
size, installed size, runtime share, application share, and baseline revision;
a raw APK, AAB, IPA, or local build-directory byte count cannot support a store
size claim by itself.

[NativePHP Mobile v4 plugins](https://nativephp.com/docs/mobile/4/plugins/introduction)
can add Swift/Kotlin, UI elements, permissions, Gradle/CocoaPods/Swift Package
Manager dependencies, repositories, assets, Android components, build hooks,
and secrets. That breadth is productive and also defines the relevant trust
boundary. PAM's signed compatibility registry is directionally stronger only
when release evidence proves the exact resolved native graph and build-hook
authority on both platforms; catalog metadata alone is not a security claim.

Servo now publishes an initial
[crates.io LTS](https://servo.org/blog/2026/04/13/servo-0.1.0-release/), whose
official announcement explicitly says the embedding API remains pre-1.0 and
monthly releases may break it. Its official
[release stream](https://servo.org/blog/) continues to report rapid work on
embedding, focus, forms, keyboard navigation, DevTools, media, and Web
compatibility. PAM should therefore keep the engine pinned and its support
statement narrow. Passing compilation is necessary but insufficient: the
release gate needs real navigation, form/input, accessibility, crash recovery,
and representative Web-compatibility journeys on the exact packaged host. The
starter UI names the pinned LTS channel without hardcoding an engine version;
machine-readable release evidence remains the authority for the exact revision.

These findings make **clean-host, signed distribution evidence** the next P0
program. Its acceptance artifact must be machine-readable and bind integer
platform/architecture/package/result codes, exact component revisions and
hashes, installer and installed footprints, launch/first-response timing,
upgrade and rollback outcomes, dependency inventory, signing identity, and the
host image. Definitions and source compilation do not satisfy the gate; only a
successful hosted run against immutable artifacts does.

The standalone first-install bootstrap still receives its archive checksum from
the same HTTPS release channel. Its strict checksum, identity, extraction and
atomic-activation checks prove integrity and rollback, not independent publisher
authorization. Automatic self-update now has the stronger boundary: each release
publishes the already signed certification manifest as a compact target-specific
asset, official binaries pin the independently distributed evidence-key identity
at build time, and the installed binary verifies signature, target and candidate
digest before allowing the bootstrap to activate anything. Source builds without
that compiled identity fail closed. Installed SemVer is a rollback floor, and an
older signed release requires an explicit version plus `--allow-downgrade`, so a
replayed historical manifest cannot silently downgrade automatic latest. A
bounded two-key window lets one bridge release pre-authorize a successor without
ever learning trust from the manifest; unrecoverable loss before that bridge
still requires verified reinstall. The `--check` path now performs the same
signed-manifest authorization before it recommends an update. A hosted release must still prove that the
four compact assets are present and consumable before this becomes published
release evidence rather than an implemented source contract.

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
Desktop now adds deliberately opt-in command spans without exposing bridge
data. Server response lineage can continue through strict Native context import
and Desktop's authenticated local bridge. Both outbound paths are now shipped:
Desktop binds propagation to its declared HTTPS origin, while Native uses a
dedicated version `00` context revalidated by PHP and each native host. Generic
request headers cannot spoof `traceparent` or `tracestate`. This trust-boundary
design follows the [W3C Trace Context privacy guidance](https://www.w3.org/TR/trace-context/#privacy)
that cross-request correlation can expose information when context reaches an
unintended downstream service.

React Native 0.83 also added automatic fetch, XHR and image inspection to its
[Network panel](https://reactnative.dev/docs/react-native-devtools#network),
including timings, headers, previews and Performance-panel events. PAM's next
step is now shipped as host-native bounded request metadata in the existing
cross-surface timeline. Android and iOS retain only the latest eight events and
export method/status integer codes, duration, failure state and byte counts.
URLs, origins, paths, queries, headers and bodies are excluded by construction;
the CLI rejects invalid codes and counts outside the Native transport bounds.

### Competitive research refresh — 2026-08-19

NativePHP Mobile v4 materially changes the PHP-mobile baseline. Its official
[architecture description](https://nativephp.com/docs/mobile/4/architecture/about-the-new-architecture)
now specifies real SwiftUI and Jetpack Compose views driven by on-device PHP
through shared memory rather than a WebView, while its
[plugin contract](https://nativephp.com/docs/mobile/4/plugins/introduction)
packages PHP, Swift, Kotlin and a capability manifest through Composer. PAM must
therefore stop treating “native UI without a JavaScript runtime” as sufficient
differentiation. The measurable contest is now renderer correctness, explicit
trust boundaries, open compatibility metadata, reproducible native builds,
device evidence, tooling quality and performance budgets.

On Server, OpenSwoole's current documentation exposes both breadth and failure
semantics: its [protocol matrix](https://openswoole.com/docs/protocols) spans
HTTP/2, WebSocket, TCP/UDP, MQTT and gRPC, while
[server configuration](https://openswoole.com/docs/25.x/modules/swoole-server/configuration)
documents coroutine caps, HTTP 503 saturation and send-buffer backpressure.
Laravel's [Octane documentation](https://laravel.com/docs/12.x/octane) keeps
FrankenPHP, RoadRunner and Swoole behind a first-party application workflow.
OpenSwoole 26.2 added current, maximum and average event-loop lag per worker.
PAM now preserves the existing cluster/pool gauges while also exporting
current/max/sample-weighted average values per worker, generation, PID and pool;
this localizes a stall without falsely claiming its syscall cause. PAM's next
Server evidence must consequently measure overload and blocking-I/O
behavior, not only happy-path throughput, and prove Octane migration across the
same supported Laravel/PHP matrix.

[React Native's testing guidance](https://reactnative.dev/docs/testing-overview)
explicitly keeps Android/iOS end-to-end tests in the quality pyramid because
JavaScript component tests cannot prove native platform behavior. Flutter makes
the same distinction between its general
[test levels](https://docs.flutter.dev/testing/overview) and executable
[accessibility guideline tests](https://docs.flutter.dev/ui/accessibility-and-internationalization/accessibility).
PAM Mobile UI therefore retains real Android emulator and UIKit simulator gates
while joining the Composer ecosystem matrix; neither layer substitutes for the
other. Composer's documented `update --prefer-lowest` mode is now exercised as
a separate graph for Mobile UI, so its declared PAM Native lower bound is a
tested contract rather than metadata optimism.

GitHub documents that workflow artifacts otherwise inherit repository retention
and may remain for up to 90 days, while `actions/upload-artifact` supports a
shorter per-artifact `retention-days` contract. PAM now classifies artifacts by
purpose: one day for cross-job prerequisites, seven days for diagnostics and
intermediate release archives, and 30 days for reproducible evidence. Durable
packages remain GitHub Release assets. A repository verifier makes an omitted or
overlong lifetime a CI failure instead of allowing storage growth to return
silently.

## Current product audit

### PAM runtime and CLI

**Shipped in this repository**

- Embedded persistent PHP runtime with supervised workers.
- HTTP/1.1, HTTP/2, HTTP/3, WebSockets, streaming, health, metrics, diagnostics,
  profiling, tracing, reload, and watchdog behavior.
- Laravel and Octane integration with executable integration tests.
- Client disconnects cancel queued or suspended PHP dispatch immediately and
  publish a standalone/cluster Prometheus counter instead of consuming the
  complete request timeout after the caller disappears.
- Project discovery, guided initialization, doctor/repair, generators, ecosystem
  capabilities, builds, packages, release checks, shell completion, and editor
  support.
- Reproducible benchmark and soak protocols.

**Needs stronger evidence**

- Public clean-host installation and upgrade journeys.
- Competitive benchmark artifacts generated from immutable releases.
- Long-duration memory, cancellation, and worker-recovery artifacts; immediate
  disconnect cancellation is covered locally, but hosted soak evidence remains
  open.
- Windows behavior and common container/orchestrator deployment matrices.
- Hosted clean-target evidence that external consumers validate the embedded
  structured diagnostics schema across Runtime, Native, Desktop, and Product.

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
- Hosted frame-time, startup, memory, binary-size, and reload-latency evidence.
  Explicit streamed release-package safety ceilings now gate all four Native
  distributables before attestation/upload; device baselines and reload latency
  remain open.
- Android hot reload activation is transactional: malformed, truncated,
  duplicate-path or traversal bundles cannot replace the last-known-good PHP
  application, and interrupted swaps recover the preserved version. End-to-end
  Android now measures accepted-version-to-first-native-frame latency (or
  failure) with bundle bytes in DevTools; hosted device distributions and p95
  release budgets remain the evidence gap.
- Screenshot/golden tests and accessibility conformance evidence.
- A public compatibility registry for plugins and PAM runtime releases.
- A cohesive, production-grade design system and visual component workbench.

**Newly shipped evidence contract**

- Shared semantic light/dark design tokens are consumed by generated Native and
  Desktop adapters with bounded, fail-closed parsing.
- Native and Desktop screenshot commands produce project-scoped PNGs, while a
  dependency-free semantic verifier measures six visual anchors.
- Product release manifests now bind both mode reports, all four captures, and
  the exact token digest. Partial evidence and post-package tampering fail
  closed; manifests without visual evidence remain backward compatible and do
  not imply visual certification.

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
6. Certify signed distribution as one cross-surface contract: immutable inputs,
   dependency inventory, installer/installed size, first success, upgrade,
   rollback, signing identity, and exact clean-host image.

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
4. PAM Desktop hardening, signed installers, clean-host updater recovery,
   permission audit, and platform CI.
5. A flagship application using PAM Server, Native, Desktop, and shared packages.

### P3 — Ecosystem scale

1. Stable extension APIs with deprecation windows and migration tooling.
2. Public compatibility and performance dashboards.
3. Curated templates for real product categories.
4. Contributor certification fixtures and plugin conformance suites. The
   portable `pam-native-plugin conformance` suite is implemented with a
   versioned, schema-valid report and deterministic PHP/Kotlin/Swift generation
   evidence; hosted Kotlin/Swift compiler and signed device certification stay
   as separate release gates.

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
| Product audit, competitor map, metrics, roadmap | `a573d19` | Covered by the cross-product audit | Covered by the cross-product audit | Shipped |
| Project-scoped bounded development artifacts | Cleanup contract `b69a23d`; measured 9.51 GB reclaimed across the three local product workspaces; every `pam dev` now enforces a fail-closed 8 GiB maximum with a reducible 256 MiB floor and exposes an enum-backed budget state through `pam info`. Runtime installation now also retains only the active release plus two recognized previous releases per platform, preserving rollback without indefinite version growth | Android/iOS generated hosts and Cargo targets use the same scoped command; sensitive `.pam` configuration is excluded from automatic deletion | Host cache retention `f895be8`; Cargo target uses the shared cleanup contract and Desktop provenance survives cleanup | Shipped |
| Bounded CI artifact retention | Runtime CI policy and verifier | Native prerequisites expire after 1 day; diagnostics and release intermediates after 7 days | Release evidence follows the shared 30-day ceiling | Implemented; workflow certification pending |
| Verifiable benchmark evidence | `f37716a`, workflow `0c4a2f7`; controlled overload suite proves bounded `503`/`Retry-After` and post-burst recovery independently from happy-path throughput; manager-recovery suite 5 measures every `SIGKILL` recovery, p50/p95/maximum latency, success and daemon RSS growth with SHA-256-bound artifacts | Manifest `742e0e4` | Attested package reproducibility `acc3a7c`; authenticated footprint and 5% release-baseline gate `2dad206` | Harnesses and portable verification shipped; public clean-host runs depend on CI execution |
| Native release-package budgets | N/A | `d00cc75`, `db9d864`, `5ca4b3b`; integer artifact codes cover iOS source, Android renderer, Android plugin API and PHP SDK archives; all four artifacts are constructed twice and must match byte-for-byte, including an isolated clean Gradle rebuild of the AAR with canonical archive ordering/timestamps; bounded schema 1 reproducibility reports record integer results, size and SHA-256, are provenance-attested beside the package, and the final job re-hashes all downloaded artifacts before streaming package-budget reports are independently reverified and GitHub publication can proceed | N/A | Portable producers/consumers, byte-reproducibility gates, strict schemas and end-to-end release evidence wiring implemented; first hosted release measurements remain pending |
| Structured error and automation contracts | `7d9fff3`; every CLI error now carries an enum-backed code, message, remediation, exact `verificationCommand` and process exit code in JSON, with the same fix/verify pair in human output; `pam catalog --json` exposes the single versioned authority behind help, completions and generated docs with sequential integer group codes and machine-readable JSON support flags; its embedded Draft 2020-12 schema and dependency-free `catalog --validate` consumer reject oversized/symlink inputs, unknown fields, duplicate commands and code/label drift offline; `catalog --compat` blocks command removal, group movement and loss of JSON support with sequential integer change codes while permitting additive evolution; actionable Doctor target/artifact/remediation report `f5709ce`; its strict schema is likewise embedded and validated offline with additive `schemaVersion` compatibility; CI produces, consumes and seals direct-target and project reports with seven-day retention | Contextual CLI commands inherit the same project/action/artifact envelope | Desktop retains its typed bridge errors and contextual diagnostics; `desktop host:doctor` now distinguishes signed-registry, explicit, sibling and PATH sources with sequential codes and verifies persisted provenance, exact artifact digest and binary identity without downloading or launching an app | Implemented across all six project codes with offline producer/consumer gates; first hosted evidence artifact remains open |
| Privacy-safe support handoff | Bounded `pam support` JSON, path redaction, payload digest, opt-in private persistence and overwrite refusal; explicit `--manager` adds a separately hashed/redacted process health and resource snapshot while continuing to exclude log contents, environment and network data; CI independently checks the digests, privacy flags, 512 KiB ceiling, private mode and adversarial source/environment non-disclosure | A generated Product workspace exercises the Native context through the shared Doctor envelope without reading application files | The same generated workspace exercises the Desktop context through the shared Doctor envelope | Implemented with clean-host direct-target and Product evidence retained inside the existing seven-day Doctor artifact; first hosted run remains pending |
| Development lifecycle event protocol | `2b724e4` | Android/iOS hosts emit schema 1 | `392a6eb` | Shipped |
| Versioned DevTools snapshots | `870e4fd` | `d4dec09` | `ea34d52` | Shipped |
| Cross-surface observability | Prometheus/control-plane metrics, including compatible current lag plus maximum and sample-weighted average at cluster, pool and exact worker/generation/PID granularity; versioned bounded health probes and `/diagnostics` worker snapshots; `pam top --json` NDJSON automation without Prometheus text scraping; non-loopback control planes fail closed without a strong environment or hardened secret-file Bearer whose digest stays in the master and whose plaintext is removed from PHP workers; structured access logs and W3C server-child trace lineage `ab0d0da`; redacted Chrome/Perfetto timeline exporter `4aff197`; bounded OTLP/HTTP JSON server spans `93aec7a`; signed official-Collector certification and evidence `5e51c03`; validated Native network trace export `58f9d4d` | Certified traces/logs/metrics `ab4805e`; strict Server context import with preserved sampling and Collector-proven parent lineage `622df52`; exact-origin outbound Native HTTP propagation `e041449`; bounded/redacted network diagnostics `bad65bd` | Explicit-opt-in command spans and signed Collector CI `789a1f1`; authenticated bridge continuation `d93c63b`; renderer trace-header spoofing closed `0b01551`; exact-origin outbound injection `e2c7840` | End-to-end lineage, scoped outbound propagation, worker-level stall localization and bounded Native network timeline events shipped across all three surfaces |
| Contextual live snapshot transport | Desktop routing `cf2609f`; Android routing `00b4df5`; iOS routing `9cf68fa` | Privilege-gated Android export `5b4b5f5`, including bounded explicit serial selection that pins every ADB step to one authorized physical/wireless target; app-scoped iOS Simulator export and generated overlay `8a95f55` | Authenticated development session `47b489b` | Server, authorized physical/emulated Android, iOS Simulator and Desktop shipped; physical-device iOS export remains excluded pending a portable Apple-supported extraction contract |
| Visual capture foundation | `42a4a07`; dependency-free semantic PNG evidence verifies CRC/filter/decompression bounds, token hash and measurable theme anchors across integer surface codes | Scoped Android/iOS PNG capture | Pixel-normalized golden harness `361287d`; protocol 6 capture remains an explicit platform-driver responsibility | Semantic parity verifier and adversarial CI fixtures shipped; reusable Android/Desktop capture jobs and their fail-closed cross-surface aggregator are implemented pending a hosted run |
| Accessible adaptive design tokens | Desktop starter run-green identity and WCAG/forced-color gate `f58a2ba`; Product workspace publishes a versioned, fail-closed semantic token contract with light/dark integer mode codes, 4/8 spacing, bounded motion and a 48-unit touch floor | Native tokens `b07d09f`; contrast-gated PAM Mobile UI themes and Studio `2597bd6`; generated Product app validates and adapts the shared token document over framework theme defaults; iOS source CI makes a validated simulator screenshot mandatory; a pinned API 36 workflow builds a clean Product workspace and requires Android light/dark captures | Authenticated worker validates the bounded shared document; renderer validates the response, applies semantic variables and follows system theme changes with a safe fallback; a pinned Servo 1.2.1/Xvfb workflow captures both generated Product themes and decodes them through the host visual harness | Portable token source, adapters, verifier, mandatory iOS capture, and fail-closed Android/Desktop report aggregation are implemented; the first clean hosted certification run remains pending |
| Cross-surface release authority and recovery | iOS audit artifact workflow `8e5e1fc` | Native release audit `58fe1d8`; source, device-host and aggregate plugin tag gates `9da8325` | Permission policy `46fead6`; interrupted updater recovery `e82efd6`; source and macOS/Windows tag gates `bd8ec18`, `20395fd`, `ac93248`; pinned Rust 1.88 and reproducible Servo patch `fd86e98`, `264ceec` | Fail-closed tag publication and native source compilation certified across Linux x64, macOS arm64 and Windows x64; signed installers and clean-machine sandbox certification remain open |
| Signed clean-host distribution evidence | Native backed enums eliminate magic status/type codes; offline schema 1 verifier streams and rehashes candidate/baseline packages, binds dependency/provenance inventories, timing, resolved host image, revisions and seven required result gates, and strictly verifies canonical Ed25519 evidence; create-new signer rejects exposed/symlink keys and zeroes seed buffers. The bootstrap requires one exact-name lowercase SHA-256 entry, computes the archive digest independently, rejects special links, bounds expansion to 4 GiB/100,000 entries, probes binary identity inside a five-second/bounded-output watchdog, stages on the destination filesystem and activates atomically | Integer Android/iOS package codes and surface/platform compatibility are executable | Native x86_64/arm64 Linux and macOS Runtime runners verify candidate/baseline attestations, load the complete bounded PHP module list without startup warnings, bind its normalized digest into the dependency inventory, and exercise recovery; Desktop adds a reusable network-disabled Ubuntu 22.04 gate for its supported portable x86-64 host plus a bounded native-installer producer/verifier that binds publisher credential and raw signature/notarization/sandbox/recovery proofs to the exact installer digest | Portable contracts and protected-key producers/verifiers implemented; first hosted artifacts plus Windows Runtime, Android, iOS, Native-app, rendered Desktop clean-session and native Desktop-installer runs remain open |
| Signed typed plugin registry | Offline schema 1 verifier, quorum rotation, rollback floor and SemVer/protocol resolver `bdb2148`; authenticated `pam add` gate `7cf672e`; exact verified-byte Composer source and bounded artifact retention `01f0273`; recoverable project rotation adoption `3bf6d08`; canonical ceremony payload, Ed25519 key identity and operational runbook `c8f9a5f`; signed cross-surface compatibility-matrix export; static verified compatibility-dashboard generator `1821bb1` | Descriptor and IDL integrity locks; signed Android runtime installer and provenance `9ddb46d` | Executable protocol/identity/hash checks; signed host acquisition, provenance and bounded retention | Installer enforcement, machine-readable evidence and an accessible zero-JavaScript dashboard generator are implemented; official multi-custodian ceremony, independent fingerprint publication and hosted catalog remain open |
| Flagship cross-surface application | Versioned readiness and idempotent check-in endpoints; deterministic three-surface release index with streaming SHA-256 and create-new evidence | PAM Mobile UI synchronizes through bounded OkHttp/URLSession and persists check-ins in the SDK's bounded offline queue before transport; the 32-entry flagship snapshot measures 8,222 bytes against Native Storage's 262,144-byte limit | The authenticated PHP worker owns a 32-item/64 KiB private-file outbox and a 24-sample/16 KiB Server history with replacement writes; the operations console exposes exact accessible values and never invents Native health | Live reads, offline mutation replay, bounded local history, and reproducible artifact aggregation span all surfaces with fail-closed integer contracts and portable JSON Schemas; authenticated Native telemetry and platform signing remain explicitly separate |

The next registry gate is intentionally operational: conduct the independent
production-key ceremony, publish the root hash through a PAM release and a
second channel, and sign the initial catalog. Composer, Android Native and the
Desktop host now enforce resolver output and persist the accepted sequence;
iOS runtime delivery remains source-integrated rather than a standalone
downloaded binary artifact.
The canonical payload, quorum rotation, revocation model and deterministic
offline tamper fixtures now exist in `bdb2148`; a locally generated private key
or unsigned hosted JSON still does not satisfy the public-registry claim.
