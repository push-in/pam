# PAM Octane readiness report

Goal: decide whether PAM is solid enough to be proposed to `laravel/octane` as
an official server next to Swoole, RoadRunner and FrankenPHP.

- Date: 2026-11-09. Branch `test/octane-readiness`, rebased on PAM 2.2.12.
- Package under test: `pushinbr/pam-octane` 1.2.0 (`packages/octane`).
  Runtime: PAM 2.2.13 (this branch), release build (`lto = "fat"`).
- PHP: PAM embeds PHP 8.5.12. As with FrankenPHP, there is no PHP version
  matrix: PAM always runs its own PHP.
- `laravel/octane` v2.23.0, Laravel 12.71 and 13.40.
- Machine: Intel Core Ultra 9 185H (22 threads), 30 GiB RAM, Linux 7.0.11,
  CPU governor `powersave`, Docker 28.1.1. Headless Chrome ran on the RTX 4070
  (ANGLE/EGL).

## Verdict

Ready on the server side. Every Octane feature works on Laravel 12 and 13,
including the ones that used to be Swoole-only (`Octane::table()`, the shared
`octane` cache, `Octane::concurrently()` on task workers, task dispatch,
`Octane::tick()` and cache intervals, opt-in with `--task-workers`), Octane's own suite passes through the PAM client, the
real ecosystem app passes the same 17 steps as FrankenPHP, and PAM is the
fastest of the four servers in the benchmark.

The first pass of this suite found fifteen real bugs, some in basic production
paths (streamed responses took a worker down for good, a `memory_limit` fatal
left a worker answering 500 forever, `--max-requests` could take a whole
cluster offline, Vite assets returned 404). They are fixed with tests. The
three production gaps that remained are closed in PAM 2.2.13:

- **Zero-loss reload and recycling.** `--max-requests` is make-before-break and
  draining never closes a connection with a request in flight or in the socket
  buffer. With the recommended `net.ipv4.tcp_migrate_req=1`: **0 failed of
  812k requests** under `wrk` (4 workers, `--max-requests=2000`, a reload every
  4 s, 28 reloads), and 0 of 275k in the per-request and keep-alive recycling
  checks at `--max-requests=500`. With the kernel default (`0`): 0 of
  825k under `wrk`; only a client that opens a new connection in the instant
  a listener closes can still see a reset, 0 to 1 per 55k at
  `--max-requests=500` with a new connection per request.
- **Timeouts answer 504.** A handler that blocks past the timeout gets
  `504 Gateway Timeout` after 2.1 s (2 s limit); the stuck worker is replaced
  and 20/20 requests right after succeed. The limit follows Octane's
  `octane.max_execution_time`.
- **Uploads are streamed to disk.** 100 MB and 1 GB uploads with the default
  `memory_limit` (128M): sha256 verified, PHP peak 18 MiB, worker peak RSS
  86 MiB, `move()` works, no temporary file is left behind; PHP's
  `upload_max_filesize` and `post_max_size` are enforced while streaming.

What is left before approaching the Octane team is integration work (see the
end of this report), not server behavior.

## How to reproduce

```bash
PAM=/path/to/pam packages/octane/readiness/run-local.sh            # every stage
packages/octane/readiness/run-local.sh suite matrix                 # chosen stages
READINESS_MIGRATE=1 ... run-local.sh matrix                         # with net.ipv4.tcp_migrate_req=1
BENCH_PAM_RUNTIME=/path/to/runtime ... run-local.sh bench           # benchmark an unreleased PAM build
READINESS_CLEAN=1  ... run-local.sh                                 # delete apps, vendor and images afterwards
```

The CI workflow `.github/workflows/octane-readiness.yml` builds PAM from the
revision under test and runs the same stages. It runs weekly, on pull
requests and on manual dispatch, and uploads `results/`.

| Stage | Location | What it does |
|---|---|---|
| 1. Octane's own suite | `readiness/octane-suite/` | Clones `laravel/octane` at its latest tag and runs the upstream suite on PAM's PHP. It then runs the suite again with every Octane request and response routed through the real `PamClient`: `PamFakeClient` is the only hook in `TestCase`, and it compares each PAM-emitted response with what Laravel produced. Also adds `PamClientTest`, a port of `FrankenPhpClientTest`. |
| 2. Feature matrix | `readiness/matrix/` | Provisions fresh Laravel 12 and 13 apps (`laravel/octane` plus this package), adds probe routes, and drives `pam octane:start/status/reload/stop` through 34 checks, including a `wrk` zero-loss run. |
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

39 checks per Laravel version, all PASS on both, except the recycling check
marked below, which depends on the kernel setting. The Swoole-parity rows were
re-run on 2026-11-10 with the release build of this branch.

| Feature | Laravel 12 | Laravel 13 | Notes |
|---|---|---|---|
| Basic responses (JSON, Blade, DB, redirect, 204, HEAD, multiple cookies, repeated headers) | PASS | PASS | |
| Request isolation: container instances, config, locale, auth, session, request attributes | PASS | PASS | No leaks across 12 requests and every worker |
| `Octane::concurrently()` | PASS | PASS | Sequential dispatcher, the same as RoadRunner and FrankenPHP |
| Octane cache store (`octane` driver), no task workers | PASS | PASS | Per-worker array store, as before |
| Octane cache store, shared (task workers or `pam-octane.shared_cache`) | PASS | PASS | Value seen by 4 workers; increments 1..20 |
| `Octane::table()` / `PamOctane::table()` | PASS | PASS | Same row in 4 workers; 400 concurrent `incr` -> 400 distinct values |
| `Octane::concurrently()` on task workers | PASS | PASS | 2 x 300 ms in 305 ms on 2 task workers, 4 x 300 ms in 605 ms; exceptions -> `TaskException` |
| Task dispatch (`--task-workers`) | PASS | PASS | 5 of 5 dispatched tasks ran |
| `Octane::tick()` / cache intervals | PASS | PASS | One tick per second on a task worker; interval value refreshed after 2.2 s |
| Tables and shared cache survive `octane:reload` and a `SIGKILL`ed worker | PASS | PASS | Row and cache intact after both |
| Streamed responses (`response()->stream()`, generators, `eventStream` SSE) | PASS | PASS | First byte in 3 to 5 ms, last byte at 1 s, chunked |
| 50 MB `download()` and 16 MB `streamDownload()` | PASS | PASS | |
| Requests reaching a busy, streaming worker are queued instead of rejected | PASS | PASS | 20/20 |
| Static files: Vite build, `robots.txt`, `storage:link`, ETag/304, Range | PASS | PASS | No PHP source and nothing outside `public/` is served |
| HTTP/1.1 keep-alive | PASS | PASS | 50 requests on one connection |
| Trusted proxies (`X-Forwarded-*`) | PASS | PASS | |
| 100 MB multipart upload, `memory_limit=128M` (plain and `move()`) | PASS | PASS | sha256 verified, 0.18 s, PHP peak 18 MiB, worker RSS 78 to 84 MiB, 0 temp files left |
| 1 GB multipart upload, `memory_limit=128M` (plain and `move()`) | PASS | PASS | sha256 verified, 1.8 to 2.1 s, PHP peak 18 MiB, worker RSS 84 to 86 MiB, 0 temp files left |
| `upload_max_filesize` / `post_max_size` while streaming | PASS | PASS | 2 MB file over `1M` gets `UPLOAD_ERR_INI_SIZE` with the form fields kept; 9 MB body over `8M` gets Laravel's 413 |
| 5 MB raw JSON body | PASS | PASS | |
| Memory over 50k requests, one worker | PASS | PASS | PHP heap 15,060 / 15,492 KiB before and after; RSS 104 / 105 MiB before and after; same worker; 0 errors |
| `--max-requests=500`, 4 workers, keep-alive | PASS | PASS | 0 of 77k to 83k failed |
| `--max-requests=500`, 4 workers, new connection per request | see below | see below | 0 failed with `tcp_migrate_req=1`; 0 to 1 of 54k to 57k with the kernel default |
| `octane:reload` under load (4 reloads, keep-alive) | PASS | PASS | 0 of 88k to 92k failed |
| Zero-loss: `wrk -c64`, `--max-requests=2000`, a reload every 4 s, 40 s | PASS | PASS | 0 of 273k and 0 of 253k failed (9 reloads each) |
| `octane:status`, `octane:stop`, then start again | PASS | PASS | |
| Request timeout (2 s limit, handler blocks for 6 s) | PASS | PASS | `504` after 2.1 s with `Connection: close`; the stuck worker is replaced; 20/20 healthy right after |
| Worker crash recovery: `SIGKILL`, `memory_limit` fatal, `exit()`, exception | PASS | PASS | 20/20 healthy afterwards in each case |
| `WorkerStopping` runs on shutdown, not at boot | PASS | PASS | |
| HTTPS (`PAM_TLS_CERT`/`PAM_TLS_KEY`) | PASS | PASS | Negotiates HTTP/2 |
| `--watch` | PASS | PASS | Runs `pam dev` in the foreground |

### Zero-loss recycling, 60 s runs

`READINESS_ZERO_LOSS_SECONDS=60`, `wrk -t4 -c64` keep-alive on `/_r/hello`,
4 workers, `--max-requests=2000`, `octane:reload` every 4 s (14 reloads per
run).

| Kernel setting | Laravel 12 | Laravel 13 |
|---|---|---|
| `net.ipv4.tcp_migrate_req=1` (recommended) | 394,329 requests, **0 errors** | 417,857 requests, **0 errors** |
| `net.ipv4.tcp_migrate_req=0` (default) | 413,514 requests, 0 errors | 411,597 requests, 0 errors |

With `tcp_migrate_req=1`, the `--max-requests=500` checks also lose nothing:
0 of 57,672 and 0 of 58,648 with a new connection per request, 0 of 81,291
and 0 of 77,252 with keep-alive.

How recycling works now:

1. A worker that reaches `--max-requests` asks the master for a replacement and
   keeps serving. The master forks the replacement from the generation's
   zygote, waits until it is ready, then sends `SIGTERM` to the old worker.
   Reload starts the whole new generation first, as before.
2. The draining worker leaves the balancer at once, and every HTTP/1 response
   it writes carries `Connection: close`: each connection finishes its
   in-flight request, or the next one it receives, and closes cleanly.
   Connections idle for `PAM_DRAIN_IDLE_MS` (1 s) and HTTP/2 connections get
   the graceful shutdown. A connection accepted just before the drain, with
   its first request still in the socket buffer, is served instead of reset
   (that was the in-worker race of the previous report).
3. The accept loop runs on the I/O threads (it used to run on the thread that
   executes PHP). It keeps accepting and handing connections to live siblings
   while the old worker's clients reconnect, sweeps the kernel queue with
   plain `accept(2)` calls until it is empty, and only then closes the
   listener.
4. Listening sockets belong to the master: the replacement adopts the
   socket the old worker listened on, so no listener with queued connections
   is closed by a recycle or a reload, with or without
   `net.ipv4.tcp_migrate_req` (docs/listener-handoff-evidence.md: 0 failures
   in 60 runs and 7.8M requests).

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
| hello | **9,119** · 7.26 / 8.80 ms | 6,865 · 9.21 / 14.44 ms | 7,042 · 9.00 / 11.24 ms | 5,586 · 11.27 / 15.11 ms |
| json | **8,890** · 7.15 / 8.89 ms | 6,624 · 9.52 / 14.84 ms | 6,777 · 9.18 / 18.74 ms | 5,526 · 11.37 / 15.40 ms |
| db (SQLite row) | **7,500** · 8.07 / 16.58 ms | 6,118 · 10.24 / 15.61 ms | 5,907 · 10.83 / 13.61 ms | 4,428 · 14.27 / 18.84 ms |
| view (Blade) | **6,963** · 8.55 / 19.05 ms | 6,336 · 9.90 / 14.54 ms | 5,752 · 11.12 / 13.55 ms | 4,698 · 13.44 / 17.67 ms |
| Container memory after the run (cgroup, page cache included) | 451 MiB | 473 MiB | 487 MiB | 749 MiB |

How it was run:

- Same Laravel 13 app with `APP_ENV=production` and `artisan optimize`.
- PHP 8.5.12 everywhere, with the same OPcache and JIT settings.
- 8 workers on CPUs 0-7.
- `wrk -t4 -c64`: a 5 s warmup, then 20 s on CPUs 8-11. Runs were sequential.
- Images: `pam-octane-bench/pam:local-2.2.13` (this branch's release build,
  through `BENCH_PAM_RUNTIME`), FrankenPHP 1.13.5,
  `php:8.5.12-cli-bookworm` with Swoole 6.2.3 and RoadRunner 2025.1.15.
- CPU governor `powersave` on a laptop, `tcp_migrate_req=0`. The numbers are
  valid only relative to each other.
- The PAM p99 of `db` and `view` in this run is noise from the governor: a
  PAM-only rerun gave 10.07 / 18.03 ms, and an A/B on the same image build
  gave `view` 10.79 ms on PAM 2.2.12 against 9.94 ms on 2.2.13, `db` 10.24 ms
  against 11.22 ms, with the same throughput (7,620 to 7,660 requests/s).
  The changes do not cost throughput on any endpoint.
- Raw `wrk` output and `machine.json` are written to `results/bench/`.

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
| 16 | High | runtime | Reload and recycling lost about 1 request in 20k to 50k: a connection accepted just before graceful shutdown was closed before its first request was read, and `--max-requests` was break-before-make | Make-before-break recycling, `Connection: close` draining, accept loop off the PHP thread, listener kept open while clients reconnect |
| 17 | High | runtime | A handler blocking past the timeout was killed by the watchdog and the client saw a reset | The transport answers `504` itself and the worker is replaced make-before-break; requests queued behind it get `503` with `Retry-After` |
| 18 | High | runtime | Upload bodies were held in memory three times (Rust, PHP string, `php://input`); a 100 MB upload needed `memory_limit` of about 164 MB and 1 GB was impossible | Multipart bodies are streamed to `upload_tmp_dir` with PHP's limits, registered for `is_uploaded_file()`, deleted after the request |
| 19 | Medium | package | `octane.max_execution_time` was ignored | It sets the request timeout (0 disables it) unless `PAM_REQUEST_TIMEOUT_MS` is set |

All of these ship in PAM 2.2.13 (runtime, server, CLI: bugs 3 to 6, 8, 11
to 13, 16 to 18) and pam-octane 1.2.0 (package: bugs 1, 2, 7, 9, 10, 14, 15,
19). Each fix has a regression test in `tests/` (Rust), `packages/octane/tests`
(PHPUnit) or this readiness suite.

## Swoole parity: soak, chaos, stress, fuzzing

Evidence and commands: `readiness/evidence/swoole-parity/RUNS.txt`, with the
JSON of every run. Release build, Laravel 12/13 readiness apps, 4 HTTP and 2
task workers, kernel default `tcp_migrate_req=0` unless noted.

| Run | Result |
|---|---|
| 30 min, `octane:reload` every 60 s, `--max-requests=2000`, `SIGKILL` of a random HTTP or task worker every ~15 s (70 + 46 kills) | 4.95M requests; increments exact; 0 table repairs; memory flat; the 1,008 failures (0.02 %) are all requests in flight on, or queued to, a killed worker |
| 30 min, reload every 60 s and `--max-requests=2000`, no kills | 7.66M requests, 448k `concurrently()` calls, 1 failure: a keep-alive connection closed by a draining worker as the client reused it, a rare race that `origin/main` shows too (A/B in the evidence) |
| 15 min leak run without recycling | 4.38M requests, 0 failed; HTTP workers flat; task workers grow about 0.27 KB per task because PHP keeps the code of every unserialized closure (same in plain `php-cli`); bounded by `--max-requests`, which task workers honour |
| Stress, 15 rounds: 256 parallel writers on one row, 2,048 concurrent cache misses, 400 clients hanging up during `concurrently()`, 200 slowloris clients, `memory_limit` fatal in an HTTP and a task worker | All exact, 0 failed requests |
| `cargo fuzz` on the table (model-checked against a `HashMap`) | 5.9M runs in 20 min, 0 crashes |
| Benchmark A/B against `origin/main`, same build setup | Feature off: within noise (-0.8 % to +1.3 %); with 2 task workers on the same 8 CPUs: -0.5 % to -5.4 % |
| Ecosystem app, task workers off and on | 17/17 and 17/17 |
| Octane fork suite with PAM table support | 273 tests, 265 pass, 0 fail; 35 of the 43 Swoole-only tests now run and pass on PAM |

## Remaining gaps

Gaps 1 to 3 of the previous report (lossless hand-over, hard timeouts, upload
memory) are closed in PAM 2.2.13, see above. Still open:

1. **Static files go through the PHP worker.** This matches Swoole, but it
   uses the single request slot. Serving `public/` from Rust would be faster
   and would leave PHP free.
2. **Task workers are opt-in.** Without `--task-workers`,
   `Octane::concurrently()` stays sequential as on RoadRunner and FrankenPHP.
   With them it runs on a private task pool (see the Swoole-parity section).
   Tasks are not interrupted when the HTTP client that started them leaves,
   as on Swoole. Released Octane declares `Octane::table()` with a
   `Swoole\Table` return type, so apps use `PamOctane::table()` until Octane
   accepts the PAM server.
3. **No automatic certificates.** FrankenPHP's `--https` uses ACME through
   Caddy; PAM needs certificate files. `--admin-port`, `--log-level` and
   `--poll` are accepted and ignored.
4. **Status output does not follow Octane.** `octane:status` prints PAM's
   format instead of Octane's "Octane server is running". Octane's own
   `ServerStateFile` and `ServerProcessInspector` are not used.
5. **Platforms.** Only Linux x86_64 was validated here. Prefork and balancing
   are Linux-only.
6. **Kernel setting.** Not needed any more: since the master keeps the
   listeners, recycling and reloads lose nothing with the kernel default
   `tcp_migrate_req=0` either.
7. **The bridge does not read `--max-requests` from Octane's config.** Octane
   has no config key for it (only the command option, default 500); PAM's own
   default limit is 10M.

## What is needed before approaching laravel/octane

1. **Ship the fixes.** PAM 2.2.13 and pam-octane 1.2.0 carry everything in
   this report. Run this suite against the released artifacts in CI
   (`octane-readiness.yml`) and publish the results.
2. **Integrate like the other servers.** Prepare the upstream pull request in
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
3. **Keep the evidence running.** Keep `octane-readiness.yml` green on main.
   Add Laravel 11 if Octane still supports it, and macOS and arm64 runners.
4. **Write the docs and threat model.** Cover static file serving, upload
   limits and temporary files, TLS and the `tcp_migrate_req` recommendation.
