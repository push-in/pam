# Changelog

## Unreleased

## 2.0.9 - 2026-08-23

- Isolate Composer-provided executable commands from the embedded PHP ini so
  host-PHP shebangs cannot load extensions built for another module ABI.
- Expose the bundled PAM Native SDK automatically to contextual native commands.

## 2.0.8 - 2026-08-23

- Isolate the optional host-PHP inspection in `pam doctor` from PAM's bundled
  PHP 8.5 configuration, preventing cross-ABI extension warnings.
- Consume the certified PAM Native 0.8.1 engine for Android distributions.
- Generate mobile projects against the PAM Native 0.8 package line.

## 2.0.7 - 2026-08-22

- Generate new HTTP applications with the permanent
  `pushinbr/pam-http-testing` package family.
- Keep PHP 8.5 as the default generated application runtime and certify
  public Composer package installation on PHP 8.5.
- Retry transient Composer advisory-service failures while keeping dependency
  audits fail-closed after a bounded number of attempts.
- Avoid duplicate branch/PR Octane runs and serialize advisory audits to stay
  within the public security service's availability envelope.

## 2.0.6 - 2026-08-22

- Keep Linux clean-host extension certification fail-closed while recognizing
  the single deterministic timezone warning emitted before shared timezonedb
  can replace Debian PHP's system database.

## 2.0.5 - 2026-08-22

- Bundle the pinned IANA timezone database with Linux distributions so PAM's
  embedded PHP 8.5 remains warning-free on minimal hosts without system tzdata.

## 2.0.4 - 2026-08-22

- Preserve the canonical release tag across distribution-certification steps so
  signed evidence always carries a valid v-prefixed SemVer identity.
- Keep inline PHP source intact inside the isolated Linux certification shell.

## 2.0.3 - 2026-08-22

- Certify packaged extension inventories through PAM's supported inline PHP
  execution contract instead of treating PHP CLI `-m` as a PAM option.

## 2.0.2 - 2026-08-22

- Make Linux master-death certification distinguish exited zombie workers from
  live workers when container PID 1 delays reaping them.

## 2.0.1 - 2026-08-22

- Treat Linux zombie dashboard processes as stopped so containerized shutdown
  does not wait until the control-plane deadline.
- Record benchmark source identity from the explicit release environment when
  mounted CI checkouts cannot be queried safely by Git.

## 2.0.0 - 2026-08-21

- Add `pam support` for bounded, path-redacted and integrity-digested Doctor
  reports, with zero persistence by default and private create-once JSON output.
- Stop retaining the disposable release binary from ordinary CI pushes after its
  isolated bundle smoke test; distributable artifacts remain release-owned.
- Run all 26 ecosystem packages against their newest constraint-compatible
  dependency graph after a non-mutating preflight, validating committed locks
  instead of allowing stale locks to hide publication incompatibilities.
- Execute every ecosystem package on both supported PHP 8.4 and PHP 8.5 runtime
  lines before any PAM or PAM Native publication can proceed.
- Test both the newest and lowest dependency graphs permitted by every package,
  including exact Native-candidate provenance at both constraint boundaries.
- Disable Composer plugins/scripts during ecosystem resolution and fail each
  graph on security advisories or abandoned dependencies before tests execute.
- Publish a validated schema 1 compatibility artifact covering all 52
  package/PHP combinations and 104 dependency-graph executions for one PAM SHA.
- Record each package commit and SHA-256 fingerprints of both tested Composer
  locks so compatibility evidence can be reproduced at exact source boundaries.
- Record and enforce one exact Native candidate Git commit across every
  dependent package/PHP result emitted by a core tag certification.
- Bind every Native-dependent ecosystem job to the exact `pam-native-php` tag
  candidate and verify its resolved lock version before allowing publication.
- Bind manual Runtime releases and their 26-package compatibility matrix to the
  same explicitly selected immutable tag.
- Report the complete Runtime, Native and Desktop build footprint from
  `pam info` while retaining the legacy `.pam-native` JSON measurement.
- Clean only regenerable Android build/Gradle outputs and Xcode's canonical
  `.pam-native/ios/App/DerivedData` before Native development rebuilds without
  removing generated hosts, sources or evidence.
- Add bounded, non-blocking OTLP/HTTP JSON server-span export with W3C parent
  lineage, HTTPS enforcement, controlled retries, redacted attributes and
  Prometheus delivery counters.
- Add reproducible OTLP interoperability certification against a signed,
  digest-pinned official OpenTelemetry Collector with bounded, tamper-evident
  CI artifacts.
- Refresh the PAM Desktop starter with the cross-surface run-green identity,
  truthful bridge lifecycle states, WCAG contrast gates, high-contrast and
  forced-color support, and a 375 px responsive visual contract.

## 1.0.3 - 2026-08-11

- Add the optional `pushinbr/pam-octane` bridge for Laravel Octane 2.19+ on
  Laravel 12 and 13, preserving Octane's worker lifecycle over PAM's native
  Rust/Tokio HTTP transport.
- Add supervised `pam octane:start`, `octane:status`, `octane:reload` and
  `octane:stop` commands with independent route-aware worker pools.
- Add bounded public response caching, authenticated tag invalidation,
  stale-while-revalidate and bounded route metrics for Octane workloads.
- Add cooperative HTTP/Redis I/O, isolated bounded PDO execution and protocol
  fuzzing contracts used by persistent Laravel workers.
- Add a PHP 8.4 × Laravel 12/13 package matrix, native end-to-end smoke,
  public-release installation smoke and reproducible benchmark/soak gates.
- Add PAM Octane publication metadata, community templates, security guidance,
  release checklist and private vulnerability reporting instructions.
- Updated `pam init --template desktop` to the PAM Desktop 1.2
  convention-first API with `#[Desktop]`, typed method commands, listeners,
  dependency injection, explicit permissions, and typed named windows.
- Updated new desktop projects to `pushinbr/pam-desktop:^1.2` and protocol 6.

## 1.0.2 - 2026-08-10

- Make fresh API projects immediately pass `pam format --check`, `pam lint`,
  and `pam test` by installing Pint and emitting formatter-clean PHP.
- Add complete Composer publication metadata to generated applications and an
  explicit escape hatch for public-install tests run from source checkouts.
- Publish checksum-verified PHP 8.4/8.5 Android runtimes and prebuilt native
  engines; `pam doctor --fix` installs them without requiring Rust locally.

## 1.0.1

- Publish the official Composer packages under the organization-owned
  `pushinbr/*` namespace so fresh public installs work from Packagist.
- Isolate Intel macOS runtime packaging from iOS simulator cross-builds.

All notable changes are documented here. The project follows Semantic Versioning
after the first stable release.

## 1.0.0 - 2026-08-10

- Adopted the Apache License 2.0 across the PAM runtime, native SDK,
  Composer packages, examples, and publication metadata. PAM may now be used,
  modified, forked, and distributed for any purpose, including commercially,
  subject to the Apache-2.0 terms.
- Updated the `pam init --template desktop` starter to protocol 5 and
  `pam/desktop` 0.5, including a polished signed-update status surface.
- Documented cross-platform DMG/MSIX packaging, feed signing, automatic updates
  and rollback while keeping the public workflow under `pam desktop`.
- Added the interactive PAM launcher and contextual project lifecycle: init,
  doctor/fix, development, generators, packages, quality, build, signing,
  packaging, and release gates.
- Added application and Composer-package CLI extension, stable JSON discovery,
  embedded Composer/Artisan execution, and generated command documentation.
- Added the `.pam` formatter and LSP with managed VS Code, Neovim, and Helix
  integration.
- Certified PAM Native Android end to end on APIs 26 and 36, including the PHP
  Embed runtime, emulator execution, and signed APK/AAB production artifacts.
- Added the generated UIKit/Xcode host, iOS PHP 8.4 and 8.5 XCFrameworks,
  simulator execution, IPA export tooling, and extension targets for Share,
  Health, Media, Widgets, App Intents, and Live Activities.
- Published the six first-party server Composer packages under coherent `^1.0`
  constraints and added release gates that reject version/tag drift.
- Reorganized the public documentation around installing PAM first and using
  one contextual command surface across the ecosystem.

## 0.1.1 - 2026-07-23

- Rebuilt official x86_64 and ARM64 runtimes on a glibc 2.35 baseline.
- Bundled the non-system shared-library closure required by PHP Embed and the
  curated extension set, removing accidental host-library dependencies.
- Added a portable release validation path and deterministic non-interactive
  toolchain installation for private repositories.
- Restricted the supported Laravel matrix to Laravel 12 and 13.

## 0.1.0 - 2026-07-23

- Official Linux archives now ship an isolated PHP Embed runtime, curated common
  extensions and private INI configuration; installed users do not need system
  PHP, Composer or Rust.
- Added validated, least-privilege Composer package releases from the monorepo to
  six read-only distribution mirrors, with isolated histories and immutable tags.
- Added a rootless, checksum-verifying installer for release bundles that include
  PAM's private PHP Embed runtime, removing PHP/Rust toolchains from end-user setup.
- Made signal watchers robust to inherited blocked signal masks while restoring the
  previous handler, mask and asynchronous-dispatch mode during cleanup.
- Renamed the project and every public prefix to **Pam — PHP, Always in Memory**.
- Added `pam composer`, including a verified first-run Composer download executed
  by Pam's own Embed SAPI without requiring a system PHP CLI.
- Added interactive and non-interactive `pam init` presets for raw Pam, Pam API,
  API + Socket, Laravel, and Laravel + Socket, with automatic dependency install.
- Added the binary-owned `Pam\Laravel` persistent host, request-container
  sandboxes, framework state resetters, streamed/binary response conversion, a
  locked Laravel 13 compatibility contract, and an RSS isolation soak.
- Added first-class `pam artisan` with CLI SAPI identity, standard streams,
  arguments, exit codes, queue workers and scheduler compatibility.
- Expanded the Laravel contract to framework 12/13, SQLite/MySQL/PostgreSQL,
  Redis, database queues, Sanctum, Scout, Livewire, Inertia, Reverb and active
  Telescope/Pulse persistence; Horizon and Socialite run on their supported
  Laravel 12 package matrix.
- Rewired request sandboxes across Events, Eloquent, database connections,
  Bus/Pipeline, validation/translation, notifications, broadcasting, hashing,
  views and routing, with executable request-scoped injection and locale
  isolation contracts.
- Added provider-safe bootstrap requests, normalized Embed uploads, persistent
  manager warmup, session-store cleanup and automatic preparation of hardened
  Laravel storage paths.
- Removed PHP CLI and system Composer as mandatory `pam doctor` dependencies;
  platform/autoload validation now runs in Embed and CLI parity is an optional
  diagnostic when a CLI installation exists.
- Enforced one Laravel request or Socket callback per worker to prevent unsafe
  interleaving through process-global managers and facades; scale remains
  process-based through the supervised worker cluster.
- Added incremental Laravel `StreamedResponse`/`BinaryFileResponse` delivery with
  native backpressure, async callback support, disconnect cancellation, `Range`
  and `HEAD` preservation, and post-stream kernel termination.
- Added `maxResponseBytes` and `maxResponseChunkBytes` transport boundaries for
  buffered and streaming responses; over-limit streams now fail the body transport.
- Hardened worker supervision for overlapping PHP executions by retaining the
  oldest active deadline, and made aggregate metrics publish a coalesced final
  idle snapshot after traffic stops.
- Added Laravel-aware `pam dev` hot reload and a reproducible, no-benchmark-theater
  comparison protocol for Pam, FrankenPHP, and Swoole.
- Split application features into optional Composer packages, now published as
  `pushinbr/pam-api`, `pushinbr/pam-socket`, `pushinbr/pam-psr-bridge`,
  `pushinbr/pam-testing`, `pushinbr/pam-core-api`, and `pushinbr/pam-skeleton`.
- Added parameter routing, a precompiled middleware pipeline, bounded rate-limit
  state, safe Composer provider discovery, and in-memory application tests.
- Added a typed Zend-to-Tokio suspend/resume protocol with concurrent request
  Fibers, request deadlines, cancellation guards, and isolated HTTP state.
- Added native stream readiness, DNS, filesystem, process and signal operations,
  plus bounded streams, incremental HTTP/SSE and a secure HTTP client.
- Added request scopes with deterministic cleanup, sampled leak detection, native
  ABI capability discovery, diagnostics, tracing, profiling and live `pam top`.
- Added `pam build` self-contained bundles with a PHP ABI library, checksummed
  manifest, relocatable ELF lookup, and strict project/symlink boundaries.
- Expanded the locked Composer contract with Illuminate 13, Symfony HttpKernel 8,
  Slim 4, Amp, Revolt, ReactPHP, Guzzle 8, Monolog and OpenTelemetry.
- Added master/worker readiness, watchdog enforcement, restart backoff, and safe
  generational reload.
- Added a master control plane for liveness, startup, readiness, and aggregate
  Prometheus metrics.
- Added bounded request cleanup, Fiber cancellation, periodic allocator cache
  release, and an RSS soak test.
- Hardened ProcessPool limits, termination, WebSocket resume tokens, and Redis/NATS
  adapters.
- Added container, systemd, CI, audit, compatibility, and production guidance.
- Fixed successful PHP programs that explicitly call `exit(0)` (including
  PHPUnit and Pest) being reported as failures by the Embed boundary.
