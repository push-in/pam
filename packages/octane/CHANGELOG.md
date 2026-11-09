# Changelog

All notable changes to PAM Octane will be documented here.

## Unreleased

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
