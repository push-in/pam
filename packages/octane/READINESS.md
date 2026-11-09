# PAM Octane readiness report

Goal: decide whether PAM is solid enough to be proposed to `laravel/octane` as
an official server next to Swoole, RoadRunner and FrankenPHP.

- Date: 2026-11-09. Branch `test/octane-readiness` (not pushed).
- Package under test: `pushinbr/pam-octane` (`packages/octane`, 1.1.0 plus the
  fixes on this branch). Runtime: PAM 2.2.11 plus the runtime fixes on this
  branch, built from source.
- PHP: PAM embeds PHP 8.5.12. As with FrankenPHP, there is no PHP version
  matrix: PAM always runs its own PHP.
- `laravel/octane` v2.23.0, Laravel 12.71 and 13.40.
- Machine: Intel Core Ultra 9 185H (22 threads), 30 GiB RAM, Linux 7.0.11,
  `powersave` governor, Docker 28.1.1. Headless Chrome ran on the RTX 4070
  (ANGLE/EGL).

## Verdict

Most of it is ready. Every Octane feature that is not Swoole-only works on
Laravel 12 and 13. The real ecosystem app passes the same 17 steps as
FrankenPHP. PAM is the fastest of the four servers in the benchmark.

Before this branch it was not ready. Fifteen real bugs turned up, and some of
them broke basic production paths: any streamed response took the worker down
for good, a memory-limit fatal error left a worker that answered 500 forever,
`--max-requests` could take a whole cluster offline, and Vite assets returned
404. All fifteen are fixed on this branch with tests. Before talking to the
Octane team, they need to ship as a PAM release, and a few gaps listed below
need to be closed.

## How to reproduce

```bash
PAM=/path/to/pam packages/octane/readiness/run-local.sh            # every stage
packages/octane/readiness/run-local.sh suite matrix                 # chosen stages
READINESS_MIGRATE=1 ... run-local.sh matrix                         # with net.ipv4.tcp_migrate_req=1
READINESS_CLEAN=1  ... run-local.sh                                 # delete apps, vendor and images afterwards
```

The CI workflow `.github/workflows/octane-readiness.yml` builds PAM from the
revision under test and runs the same stages. It runs weekly, on pull
requests and on manual dispatch, and uploads `results/`.

| Stage | Location | What it does |
|---|---|---|
| 1. Octane's own suite | `readiness/octane-suite/` | Clones `laravel/octane` at its latest tag and runs the upstream suite on PAM's PHP. It then runs the suite again with every Octane request and response routed through the real `PamClient`: `PamFakeClient` is the only hook in `TestCase`, and it compares each PAM-emitted response with what Laravel produced. Also adds `PamClientTest`, a port of `FrankenPhpClientTest`. |
| 2. Feature matrix | `readiness/matrix/` | Provisions fresh Laravel 12 and 13 apps (`laravel/octane` plus this package), adds probe routes, and drives `pam octane:start/status/reload/stop` through 28 checks. |
| 3. Ecosystem app | `readiness/ecosystem/` | One Laravel 12 app with Livewire 3, Inertia 2 (Vue), Filament 4, Sanctum 4, Horizon 5 (Redis in a disposable container), Reverb 1, Redis queues, the scheduler, the public disk and log mail. Runs an HTTP plus headless Chrome flow under PAM Octane and under FrankenPHP. |
| 4. Benchmark | `readiness/bench/` | Public-ready harness: pinned versions, Dockerfiles, machine capture and sequential `wrk` runs. See `bench/README.md`. |

## 1. laravel/octane test suite (v2.23.0)

| Run | Tests | Pass | Fail | Skipped |
|---|---|---|---|---|
| Upstream suite on PAM's PHP 8.5.12 (plus 6 PAM client tests) | 221 | 178 | 0 | 43 |
| Same suite with all requests and responses through `PamClient` | 221 | 178 | 0 | 43 |

- 111 request/response exchanges were cross-checked. 0 mismatches in method,
  URI, host, query, input, cookies, status, body or `Set-Cookie`.
- All 43 skipped tests are Swoole-only (`Swoole*Test`, `TableTest`,
  `OctaneStoreTest` with Swoole tables, task dispatchers). They skip because
  the `swoole` extension is absent, as on RoadRunner and FrankenPHP.
- Per-test results are written to `results/octane-suite/summary.md`.

## 2. Feature matrix (Laravel 12 and 13)

Default Linux settings: every check passes except two, which pass with
`net.ipv4.tcp_migrate_req=1` on some runs (see gaps).

| Feature | Laravel 12 | Laravel 13 | Notes |
|---|---|---|---|
| Basic responses (JSON, Blade, DB, redirect, 204, HEAD, multiple cookies, repeated headers) | PASS | PASS | |
| Request isolation: container instances, config, locale, auth, session, request attributes | PASS | PASS | No leaks across 12 requests and every worker |
| `Octane::concurrently()` | PASS | PASS | Sequential dispatcher, the same as RoadRunner and FrankenPHP |
| Octane cache store (`octane` driver) | PASS | PASS | Per-worker array store. It is shared through a table only on Swoole. |
| `Octane::table()` | N/A | N/A | **Swoole-only in Octane itself** |
| `Octane::tick()` / intervals | N/A | N/A | **Swoole-only in Octane itself** |
| Task workers (`--task-workers`) | N/A | N/A | **Swoole-only in Octane itself** |
| Streamed responses (`response()->stream()`, generators, `eventStream` SSE) | PASS | PASS | First byte in 2 to 6 ms, last byte at 1 s, chunked |
| 50 MB `download()` and 16 MB `streamDownload()` | PASS | PASS | |
| Requests reaching a busy, streaming worker are queued instead of rejected | PASS | PASS | 20/20. PAM 2.2.11 answered 0/20 (all 503). |
| Static files: Vite build, `robots.txt`, `storage:link`, ETag/304, Range | PASS | PASS | No PHP source and nothing outside `public/` is served |
| HTTP/1.1 keep-alive | PASS | PASS | 50 requests on one connection |
| Trusted proxies (`X-Forwarded-*`) | PASS | PASS | |
| 100 MB multipart upload | PASS | PASS | sha256 verified, 0.36 s. Needs the limits raised (see below). |
| 5 MB raw JSON body | PASS | PASS | |
| Memory over 50k requests, one worker | PASS | PASS | PHP heap 15,452 KiB before and after; RSS 108 MiB before and after; same worker; 0 errors |
| `--max-requests` recycling under load (4 workers, limit 2000) | 2 of 44k failed | 1 of 44k failed | Was 85% failed on 2.2.11 (whole cluster down). See gaps. |
| `octane:reload` under load (3 reloads) | 1 of 48k failed | 5 of 48k failed | See gaps |
| `octane:status`, `octane:stop`, then start again | PASS | PASS | Stop followed by start failed before the fix |
| Request timeout (2 s limit, handler blocks for 6 s) | PASS | PASS | Answered in 2.4 s and the worker is replaced, but the client gets a dropped connection rather than a 504 (see gaps) |
| Worker crash recovery: `SIGKILL`, `memory_limit` fatal, `exit()`, exception | PASS | PASS | 20/20 healthy afterwards in each case |
| `WorkerStopping` runs on shutdown, not at boot | PASS | PASS | |
| HTTPS (`PAM_TLS_CERT`/`PAM_TLS_KEY`) | PASS | PASS | Negotiates HTTP/2 |
| `--watch` | PASS | PASS | Runs `pam dev` in the foreground |

With `READINESS_MIGRATE=1` (a private network namespace with
`tcp_migrate_req=1`):

- Laravel 12: reload passes (0 of 50.5k failed) and `--max-requests` loses 2
  of 45k.
- Laravel 13: `--max-requests` passes and reload loses 2 of 48.5k.

The kernel-queue resets disappear. What remains is the race described in the
gaps.

## 3. Ecosystem app: PAM Octane vs FrankenPHP 1.13.5 (same app, same PHP 8.5.12)

| Flow step | PAM Octane | FrankenPHP |
|---|---|---|
| Chrome renderer (WebGL on the NVIDIA GPU) | PASS | PASS |
| `/up` | PASS | PASS |
| Vite asset served from `public/build` | PASS | PASS |
| Inertia (Vue) page renders | PASS | PASS |
| Reverb WebSocket connects (Echo) | PASS | PASS |
| Broadcast reaches the browser through Reverb | PASS | PASS |
| Inertia client-side visit (`X-Inertia`) | PASS | PASS |
| Livewire component round-trips | PASS | PASS |
| Livewire multi-file upload (`files[]` multipart) | PASS | PASS |
| Filament admin login and dashboard | PASS | PASS |
| Horizon dashboard | PASS | PASS |
| Sanctum token authentication (200 with a token, 401 without) | PASS | PASS |
| Queued job processed by Horizon on Redis | PASS | PASS |
| Mail (log driver) | PASS | PASS |
| Scheduler (`schedule:run`) | PASS | PASS |
| File storage upload, then fetch through the public URL | PASS | PASS |
| No page errors or failed same-origin requests | PASS | PASS |

Under PAM, Horizon and Reverb run with `pam artisan`. Under FrankenPHP they run
with php-cli, because Horizon re-executes `PHP_BINARY` and
`frankenphp php-cli` cannot be used as that binary.

## 4. Benchmark

Each cell is requests/s, then p50 and p99 latency. There were 0 errors in
every run.

| Endpoint | PAM Octane | FrankenPHP | Swoole | RoadRunner |
|---|---|---|---|---|
| hello | **9,328** · 6.66 / 8.44 ms | 7,188 · 8.69 / 13.79 ms | 7,066 · 8.66 / 11.09 ms | 5,890 · 10.69 / 14.06 ms |
| json | **9,040** · 7.10 / 8.39 ms | 7,106 · 8.83 / 13.48 ms | 6,895 · 9.14 / 11.14 ms | 5,775 · 10.87 / 14.81 ms |
| db (SQLite row) | **7,816** · 7.92 / 9.99 ms | 6,233 · 10.04 / 15.05 ms | 5,985 · 12.66 / 14.67 ms | 4,961 · 12.66 / 16.80 ms |
| view (Blade) | **7,691** · 8.62 / 10.08 ms | 6,389 · 9.85 / 14.24 ms | 5,960 · 10.64 / 11.71 ms | 5,279 · 11.93 / 15.48 ms |
| Container memory after the run (cgroup, page cache included) | 411 MiB | 402 MiB | 484 MiB | 768 MiB |

How it was run:

- Same Laravel 13 app with `APP_ENV=production` and `artisan optimize`.
- PHP 8.5.12 everywhere, with the same OPcache and JIT settings.
- 8 workers on CPUs 0-7.
- `wrk -t4 -c64`: a 5 s warmup, then 20 s on CPUs 8-11. Runs were sequential.
- Images: `pam-octane-bench/pam:2.2.11` (the PAM release), FrankenPHP 1.13.5,
  `php:8.5.12-cli-bookworm` with Swoole 6.2.3 and RoadRunner 2025.1.15.
- Raw `wrk` output and `machine.json` are written to `results/bench/`.
- The numbers come from a laptop on the `powersave` governor. They are valid
  only relative to each other.

## Bugs found and fixed (all with tests)

| # | Severity | Where | Bug | Fix |
|---|---|---|---|---|
| 1 | Critical | package and runtime | Any streamed response (`stream`, SSE, `download`, `streamDownload`) wedged the worker: every later request got a 500 and the worker was never replaced. The dispatcher tore down output buffers inside the active output handler. | `Response::streamOutput()` in the runtime, used by the bridge (with a fallback for 2.2.11) |
| 2 | High | package | Generator callbacks from `response()->stream()` sent an empty body. Under `LARAVEL_OCTANE`, Laravel leaves them to the client. | Generators are iterated, like `StreamsGeneratorResponses` |
| 3 | Critical | runtime | A PHP engine fatal (`memory_limit`, bailout) left the worker answering 500 forever | The worker is recycled after an engine failure |
| 4 | Critical | runtime | `SIGTERM` killed workers instantly. The Zend handler re-raised it with `SIG_DFL`. Reload and stop cut in-flight requests, the listener quiesce never ran, and shutdown functions (`WorkerStopping`) never ran. | Termination signals are reset before Tokio installs its handlers |
| 5 | Critical | runtime | `--max-requests` recycles counted as crashes. Exponential backoff ran inside the master loop, and workers recycled together (no stagger below 1000). With the limit at 500 under load, 85% of requests failed. | No backoff after a clean exit, stagger always, quiesce on recycle, hand-off while draining. Now 0 of 53k failed at a limit of 5000, against 12 on 2.2.11 |
| 6 | High | runtime | A request reaching a worker whose slot was held by a stream got a 503 | Wait for the slot, bounded by the request timeout |
| 7 | High | package | Files in `public/` returned 404: Vite builds, `robots.txt`, `storage:link` | `PamClient` implements Octane's `ServesStaticFiles` |
| 8 | High | runtime | Multipart array fields were broken (`files[]`, `tags[]`, `a[b][c]`), which affects multi-file inputs and Livewire uploads | PHP's rules via `parse_str` and the transposed `$_FILES` layout. The parser also streams uploads instead of copying the body three times. |
| 9 | High | package | Octane's `WorkerStopping` fired while the worker booted | Moved to engine shutdown (made possible by #4) |
| 10 | High | package | The managed application was always named `pam-octane`, so `octane:stop` followed by `octane:start` failed, and so did a second project on the same machine | Per-project name, `--name`, and a fallback for the legacy name |
| 11 | Medium | runtime CLI | `pam artisan` lost CLI identity in PAM 2, so `runningInConsole()` was false. `reverb:start` and other console-only commands and publishables were missing. | `PAM_CLI_MODE` and `APP_RUNNING_IN_CONSOLE` for artisan |
| 12 | Medium | runtime CLI | PHPUnit `#[RunInSeparateProcess]` failed, because `pam -d …` could not read the script from stdin | stdin, `-` and `-f` work as in php-cli |
| 13 | Medium | runtime | `pamd` inherited the caller's pipes, so `pam octane:start \| tee` and CI log capture hung | Inherited descriptors are marked close-on-exec |
| 14 | Medium | package | The package's own test suite failed on PAM's PHP: it was only green on system PHP 8.4. Also, `pam:octane` was not registered under `pam dev`. | SAPI fast path only inside a native dispatch; the command is registered under every SAPI |
| 15 | Low | package | Octane flags (`--workers=4`, `--max-requests=500`, `--host=`, `--watch`) were rejected | Octane spellings accepted; `pam up --max-requests` added; TLS through the environment |

Commits on `test/octane-readiness`:

| Commit | Change |
|---|---|
| `9bf7abeb` | runtime: shutdown and recycling |
| `c6b9f539` | runtime: streaming and multipart |
| `c20187fc` | CLI: stdin and `-f` |
| `a3d3e32f` | package: streaming, `WorkerStopping`, names, options |
| `d52b5230` | package: static files |
| `3dc93ed1` | CLI: artisan identity |
| `3e6e5cab`, `593fc314` | server: slot queueing |
| last commit on the branch | the readiness suite and this report |

## Remaining gaps

1. **Hand-over is not fully lossless yet.** A reload or a recycle still loses
   about 1 request in 20k to 50k under constant load.
   - With default kernel settings, part of the loss is the kernel resetting
     connections queued on a closing `SO_REUSEPORT` listener. Setting
     `tcp_migrate_req=1` removes that part. It is documented, and CI sets it.
   - The rest is a race inside the worker. A connection accepted just before
     graceful shutdown is closed by hyper before its first request is read.
   - What would close it:
     - Serve the first request of connections accepted while draining.
     - Or recycle make-before-break: the master starts the replacement before
       the old worker stops listening, as reload already does.
     - Or have the master own one shared listener.
   - Today `--max-requests` is break-before-make. With very low limits and few
     workers, capacity dips. PAM's own default limit is 10M, while Octane's is
     500; the bridge does not read `octane.max_requests`.
2. **Hard timeouts drop the connection.** A handler that blocks past the
   deadline is killed by the watchdog and the client sees a reset. It should
   receive a 504. `octane.max_execution_time` is not mapped to
   `PAM_REQUEST_TIMEOUT_MS`.
3. **Upload bodies are held in memory.** The body is buffered in Rust and in
   PHP. A 100 MB upload needs `PAM_MAX_BODY_BYTES`, `upload_max_filesize`,
   `post_max_size` and `memory_limit` of roughly the body size plus 64 MB.
   FrankenPHP and RoadRunner stream uploads to disk.
4. **Static files go through the PHP worker.** This matches Swoole, but it
   uses the single request slot. Serving `public/` from Rust would be faster
   and would leave PHP free.
5. **`Octane::concurrently()` is sequential**, as on RoadRunner and FrankenPHP.
   PAM has Fibers and process pools and could offer a real `DispatchesTasks`.
6. **No automatic certificates.** FrankenPHP's `--https` uses ACME through
   Caddy; PAM needs certificate files. `--admin-port`, `--log-level` and
   `--poll` are accepted and ignored.
7. **Status output does not follow Octane.** `octane:status` prints PAM's
   format instead of Octane's "Octane server is running". Octane's own
   `ServerStateFile` and `ServerProcessInspector` are not used.
8. **Platforms.** Only Linux x86_64 was validated here. Prefork and balancing
   are Linux-only.

## What is needed before approaching laravel/octane

1. **Release the fixes.** Ship PAM 2.2.12 with the runtime fixes and
   pam-octane 1.2 with the package fixes. Run this suite against the released
   artifacts and publish the results.
2. **Close gaps 1 and 2.** Make recycling lossless, ideally make-before-break
   with a hard-timeout 504. Map `octane.max_requests` and
   `octane.max_execution_time`.
3. **Integrate like the other servers.** Prepare the upstream pull request in
   the shape of the FrankenPHP integration:
   - `octane:start --server=pam` through a `StartPamCommand`.
   - `octane:install --server=pam`, which downloads and verifies the PAM
     binary the way FrankenPHP is downloaded: stable URLs, checksums and
     signatures.
   - `Laravel\Octane\Pam\{PamClient, ServerProcessInspector, ServerStateFile}`
     so that `octane:status/reload/stop` behave like the other servers.
   - `octane.server=pam` in the config stub.
   - A CI job in the Octane repository that installs the PAM binary.
   - Docs.
   - Today PAM can only plug in from outside, because `StartCommand` hardcodes
     its list of servers. A small driver-registration hook in Octane is the
     alternative to ask for.
4. **Keep the evidence running.** Keep `octane-readiness.yml` green on main.
   Add Laravel 11 if Octane still supports it, and macOS and arm64 runners.
5. **Write the docs and threat model.** Cover static file serving, upload
   limits, TLS and the `tcp_migrate_req` recommendation.
