# Changelog

All notable changes are documented here. The project follows Semantic Versioning
after the first stable release.

## Unreleased

## 0.1.22 - 2026-07-25

- Staged incoming navigation routes before the first rendered transition frame,
  removing destination flashes while keeping animations and back navigation smooth.

## 0.1.21 - 2026-07-24

- Fixed image and decorative child touches inside nested `Pressable` cards,
  while preserving horizontal scrolling and independently interactive controls.

## 0.1.20 - 2026-07-24

- Added rich-content virtual lists and 12-column virtual grids with stable,
  granular RecyclerView updates for images, controls and arbitrary components.
- Added event coalescing, portable bundle verification, boot-safe lifecycle
  queues, performance CI budgets and physical-device baseline evidence.
- Added typed route parameters, deep links, stack operations, new shared-axis
  transitions and hardened Android back behavior.
- Added the debug-only on-device DevTools overlay and `pam mobile devtools`
  command with live FPS, mount/decode cost, commit and heap metrics.
- Hardened PamUI grids, tabs, overlays, inputs and accessibility across Android
  12 and Android 16, and removed the remaining legacy UI nomenclature.

## 0.1.19 - 2026-07-24

- Connected Android's system back button and back gesture to `Navigator`
  automatically.
- Pop the native stack from secondary routes and close the Activity normally
  from its root route.
- Added opt-out support for applications with a custom back handler.

## 0.1.18 - 2026-07-24

- Prevented normal hot-reload transport disconnects from opening the fatal
  Android error overlay when a debug app runs without the dev server.
- Kept protocol and bundle validation failures visible to developers.

## 0.1.17 - 2026-07-24

- Added the native animated stack navigator and fluent `Router`.
- Added push, pop, replace, and reset transitions rendered on Android's UI
  thread.
- Added slide, bottom sheet-style, fade, scale, and no-animation presets with
  RTL mirroring and reduced-motion support.

## 0.1.16 - 2026-07-24

- Added the explicit rich-content `Grid` container for images, pressables,
  nested layouts and custom PAM components.
- Kept responsive spans, offsets, ordering and independent gutters available
  through both tags and the typed tree API.
- Updated the bundled Pam Native Composer package identity from the stale
  `0.1.1` development version to `0.1.16`.

## 0.1.15 - 2026-07-24

- Added a native responsive grid engine with configurable column tracks,
  mobile-first spans, offsets, ordering, horizontal and vertical gutters.
- Added declarative grid attributes, Bootstrap-style utility classes and the
  equivalent typed PHP `Style` API.
- Made rich grid content measure and reflow correctly inside scroll views,
  orientation changes, tablets, split-screen layouts and foldables.
- Documented the complete grid and flex authoring model and added synchronized
  PHP, Rust and Android protocol coverage.

## 0.1.14 - 2026-07-24

- Added integer `v-for` sources so templates can repeat content directly with
  `v-for="$item in $count"` and receive one-based loop values.
- Kept the Android process alive across PHP hot reloads while remounting fresh
  component state and restoring every native event and module callback.
- Disabled request execution deadlines for the persistent mobile PHP runtime.
- Debounced mobile source changes to avoid publishing partial or duplicate hot
  reload bundles while editors are still writing files.
- Fixed native clipping and retained layout behavior used by repeated content.

## 0.1.13 - 2026-07-24

- Fixed mobile bundle traversal so nested generated, hidden, build, test and
  documentation trees cannot inflate or invalidate production APK assets.
- Completed responsive PamUI grid, compound control, overlay, list and dark
  theme hardening validated by 40 physical-device instrumented tests.
- Propagated Android system appearance through the native runtime and PHP
  window metrics, including live light/dark changes without an app restart.
- Kept PamUI semantic theme classes, component colors and Android system bars
  synchronized while preserving custom light and dark palettes.
- Cleared the strict PHPStan level 9 findings discovered by the release CI.

## 0.1.12 - 2026-07-24

- Refreshed the bundled Pam Native showcase lock after the public package
  identity migration and pinned the release to its fully verified source.

## 0.1.11 - 2026-07-24

- Rebranded the complete mobile component surface as PamUI while retaining only
  the third-party notices required by the original MIT license.
- Hardened Android layout, font scaling, intrinsic measurement, overlays,
  scrolling, grids, compound inputs, Markdown and retained native rendering.
- Refined the mobile catalog theme and responsive recipes, and verified the
  production host with Rust, PHP, Android unit/lint and 40 physical-device
  instrumented tests under strict UI-thread frame budgets.

## 0.1.10 - 2026-07-24

- Redesigned the generated desktop starter as a responsive runtime control
  surface with a clear PHP-to-Rust-to-Servo pipeline.
- Translated the complete starter experience to English, including PHP
  responses, native shell labels, runtime states, and the secondary inspector.
- Added focused tablet and mobile layouts, accessible controls, visible focus
  states, and reduced-motion support.

## 0.1.9 - 2026-07-24

- Bundled verified PHP Android runtimes and precompiled PAM Native engines for
  `arm64-v8a` and `x86_64` in every release.
- Added release gates that reject missing Android libraries, headers, provenance
  metadata or native engines before publication.
- Fixed official mobile UI resource autolinking so Android generates its
  localized `R` class and compiles UI-enabled applications.

## 0.1.8 - 2026-07-24

- Updated the desktop starter to the concise `Application::make(...)` DSL.
- Replaced entry-path terminology in generated windows with the more expressive
  `Window::load(...)` API while keeping the complete native desktop showcase.

## 0.1.5 - 2026-07-23

- Made `pam desktop` discover the Composer-provided `vendor/bin/pam-desktop`
  launcher from the target project, including custom Composer vendor directories.
- Improved missing-host guidance so desktop projects can be repaired with
  `pam composer install` without adding a system PHP, Node or Rust runtime.

## 0.1.4 - 2026-07-23

- Redesigned the CLI with a cohesive terminal-native visual system, semantic
  status indicators and automatic color support that respects `NO_COLOR`.
- Added grouped command discovery and focused `pam help <command>` documentation
  with options and executable examples.
- Upgraded project initialization, diagnostics, hot reload, server startup,
  telemetry, production builds and benchmark reports with clearer hierarchy and
  actionable feedback while preserving clean output in pipes and CI.

## 0.1.3 - 2026-07-23

- Centralized all first-party Composer coordinates and added regression checks
  that reject the retired `pam/*` package names.
- Blocked binary publication until every matching package version is available
  and installable from the public Packagist index.
- Made generated package mirrors converge safely on canonical subtree history
  while preserving immutable release tags.
- Added complete generated-manifest assertions and strict publication metadata
  to new Pam API projects.

## 0.1.2 - 2026-07-23

- Published the first-party Composer packages under the organization-owned
  `pushinbr/pam-*` namespace.
- Updated generated projects, compatibility fixtures and package documentation
  to use the public Packagist coordinates.

- Updated the `pam init --template desktop` starter to stable public API 1,
  protocol 6 and `pushinbr/pam-desktop` 1.0 with PHP plugins, native menus, tray,
  close-to-tray, global shortcuts and supervised background jobs.
- Added an extension-runtime surface to the starter and documented
  process-isolated Rust plugin scaffolding under `pam desktop plugin`.
- Focused current desktop generation and package documentation on
  self-contained Linux x86-64 delivery while preserving the public workflow
  under `pam desktop`.
- Added explicit API-version negotiation to the generated UI and runtime
  diagnostics so incompatible hosts fail visibly before application calls.

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
- Split application features into optional Composer packages: `pushinbr/pam-api`,
  `pushinbr/pam-socket`, `pushinbr/pam-psr-bridge`, `pushinbr/pam-testing`, `pushinbr/pam-core-api`, and the
  `pushinbr/pam-skeleton` project.
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
