# Changelog

All notable changes to PAM Octane will be documented here.

## 1.4.0 - 2026-11-11

Laravel Octane will not ship PAM itself (laravel/octane#1188 was closed in
favour of a package), so this release makes the package the full replacement
for that pull request. No breaking changes; requires PAM 2.2.15 for
`--server=pam`.

- `php artisan octane:start --server=pam` (or `OCTANE_SERVER=pam`), plus
  `octane:reload`, `octane:stop` and `octane:status`, with stock
  `laravel/octane` 2.19+. The service provider binds Octane's command classes
  to small subclasses that answer for `pam` and hand every other server back
  to Octane. `octane:start` runs a foreground `pam start` cluster supervised
  like Octane's other servers (request log, `--watch`, `Ctrl+C`), with
  `--workers`, `--task-workers` and `--max-requests`; `octane:reload` is a
  zero-downtime `SIGHUP` reload. Also through `pam artisan`.
- `php artisan octane:install --server=pam` (and `pam` in the server prompt)
  finds the `pam` binary or, after confirmation, installs it with PAM's
  official install script, warns when PAM is older than 2.2.15, and points
  Windows users to WSL 2 or Docker.
- `Octane::table()` returns the PAM shared-memory table inside PAM workers, so
  code written for Swoole's tables runs unchanged (`PamOctane::table()` stays).
- `octane.serve_static_files` and `octane.static_file_headers` are honoured.
- `PamServer::listenWithApplicationConfig()` reads `pam-octane.server` from
  the booted application; `PamServer::serverOptions()` validates it.
- Tests: the fork's PAM server tests (binary requirements, process inspector,
  state file), Octane's 35 Swoole-only `TableTest` and `OctaneStoreTest`
  ported to PAM tables, the commands on Octane 2.19 and 2.24 with Laravel 12
  and 13, `tests/tools/octane-suite.sh` (laravel/octane's own suite with this
  package installed: 260 tests, 0 failures, 112 exchanges cross-checked
  through the PAM client) and `tests/tools/e2e-smoke.sh` (a fresh application in
  a container: install, start, a request, reloads under load with no failed
  request, stop). CI runs them on every pull request.
- The test suite no longer needs PHPUnit process isolation, which PAM 2.2.15
  cannot run under PHPUnit 12 (Laravel 12).

## 1.3.0 - 2026-11-11

Requires PAM 2.2.15 for the edge page cache, the post-boot zygote, shared
tables, task workers and ticks; the package keeps working on 2.2.11+ (the
page cache headers are then set and simply not acted upon).

- With PAM's post-boot zygote (the default for `octane:start` on Linux),
  workers are forked from a booted Laravel application. The bridge closes the
  database and Redis connections of both the console and the Octane worker
  application before the fork (`PamServer::registerForkHooks()`), so every
  worker opens its own connections lazily. Task workers forked from the
  booted application take the task handler per request.
- `Pam\Octane\Http\Middleware\CacheResponse` (alias `pam.cache`) marks
  routes for PAM's edge page cache with a TTL, stale window and tags
  (`product:{product}` resolves route parameters).
- `Pam\Octane\Facades\Pam::purge()`, `purgeUrl()` and `purgeAll()`, and the
  `PurgesPageCache` model trait that purges a model's tags after commit.
  Purges from queue workers and Artisan use the invalidation log.
- New `PAM_RESPONSE_CACHE*` settings in `config/pam-octane.php`.
- Octane tables (`octane.tables`) in shared memory: `PamOctane::table($name)`
  (and `Octane::table()` on Octane versions with PAM table support).
- The `octane` cache store is shared by every worker when task workers run, or
  with `PAM_OCTANE_SHARED_CACHE=true`; otherwise it stays per worker.
- `--task-workers` (or `PAM_OCTANE_TASK_WORKERS`) for `Octane::concurrently()`,
  task dispatch and `Octane::tick()` / cache intervals.
- Readiness: Octane's table and cache tests run against PAM tables, the matrix
  checks every feature, and `tools/parity_soak.py` runs soak, chaos and leak
  tests for them.

## 1.2.0 - 2026-11-09

Requires PAM 2.2.13 for the transport fixes (streamed uploads, 504 on timeout,
zero-loss recycling); the package keeps working on 2.2.11+.

- The request timeout follows Octane's `octane.max_execution_time` (seconds,
  `0` disables it); `PAM_REQUEST_TIMEOUT_MS` overrides it. Requests past it
  are answered `504 Gateway Timeout` and the worker is replaced.
- Uploads are streamed to disk by PAM 2.2.13: `PAM_MAX_BODY_BYTES` only bounds
  bodies held in memory, PHP's upload limits apply to multipart, and
  `memory_limit` no longer needs to cover the upload size.
- Laravel Octane readiness suite (`readiness/`): Octane's own test suite,
  the feature matrix on Laravel 12 and 13, an ecosystem app and a benchmark
  harness, with `READINESS.md`.
- Fix: files in `public/` (Vite builds, `robots.txt`, the `storage:link`
  symlink) returned Laravel's 404. PamClient now implements Octane's
  `ServesStaticFiles` like the Swoole client, with MIME types, ETag/304,
  Range requests and `static_file_headers`; PHP sources, dotfiles and paths
  outside `public/` (other than the storage symlink) are never served.
  Disable with `PAM_SERVE_STATIC_FILES=false` when a proxy serves them.
- Fix: any streamed response (`response()->stream()`, SSE, `download()`,
  `streamDownload()`) poisoned the worker; every later request answered 500.
- Fix: generator callbacks from `response()->stream()` (Laravel keeps them
  untouched under Octane) are iterated instead of sending an empty body.
- Fix: Octane's `WorkerStopping` fired while the worker booted; it now fires
  when the worker stops.
- Fix: the managed application was always named `pam-octane`, so a second
  project could not start and `octane:stop` followed by `octane:start` failed.
  Names are now unique per project (`--name` / `PAM_OCTANE_NAME` override);
  control commands fall back to the legacy name.
- `pam octane:start` accepts Laravel Octane's option spellings
  (`--host=`, `--port=`, `--workers=`, `--max-requests=`, `--watch`) and
  ignores server-specific ones (`--server`, `--log-level`, `--rr-config`,
  `--caddyfile`, `--admin-port`, `--task-workers`, `--poll`).
- `--watch` runs the foreground PAM development watcher.
- The `pam:octane` command is registered under every SAPI (`pam dev`).
- TLS via `PAM_TLS_CERT` / `PAM_TLS_KEY`.
- The response emitter only uses the Embed SAPI fast path inside a native
  dispatch, so the package suite passes on PAM's own PHP.

## 1.0.3 - 2026-08-11

- Bridge PAM's native HTTP transport directly to `Laravel\Octane\Worker`.
- Preserve streamed responses, downloads, cookies, headers and output buffers.
- Add package discovery and the `pam:octane` Artisan command.
- Add configurable request and response safety limits.
- Add route-aware worker pools, bounded response caching and route metrics.
- Certify PHP 8.4 with Laravel 12 and 13.
- Add native runtime smoke, release-install smoke and community support contracts.
