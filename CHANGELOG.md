# Changelog

All notable changes are documented here. The project follows Semantic Versioning
after the first stable release.

## Unreleased

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
- Split application features into optional Composer packages: `pam/api`,
  `pam/socket`, `pam/psr-bridge`, `pam/testing`, `pam/core-api`, and the
  `pam/skeleton` project.
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
