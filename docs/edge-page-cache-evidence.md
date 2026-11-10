# Edge page cache: validation evidence

Branch `feat/edge-page-cache`, PAM 2.2.14 + edge page cache, local workstation
(22 threads, Linux x86_64), 2026-11-10. Runtime: release build of this branch
(`cargo build --release`, PHP 8.5). Apps: Laravel 12 and Laravel 13 readiness
apps (`packages/octane/readiness`), `pushinbr/pam-octane` from this branch.

## Rust and PHP test suites

| Suite | Result |
| --- | --- |
| `cargo test` (all targets) | 273 passed, 2 ignored; 2 `editor::` unit tests need the untracked `pam-native/` SDK dir and pass with `PAM_NATIVE_HOME` set (environment, unrelated) |
| `tests/page_cache.rs` (integration, real PHP worker) | 15/15, then 15 consecutive green runs after the readiness-probe fix |
| `cargo clippy --all-targets -D warnings` | clean |
| `pam-octane` PHPUnit | 28 tests, 144 assertions, OK |
| `pam-octane` PHPStan | no errors |

## Fuzzing (`fuzz/fuzz_targets/page_cache_policy.rs`)

Request-key, response-storability and purge-URL policy. Two earlier rounds
found real bugs, fixed in this branch (unparsable `Pragma`/`Vary` accepted;
purge URLs with non-visible bytes did not round-trip the invalidation log).
Final round on the fixed policy: 3 workers x ~10 min, ~9.4 M executions,
coverage plateau at 2818 edges, no crash, leak or timeout.

## Readiness matrix, cache ON and OFF

`READINESS_RESPONSE_CACHE=1|0 matrix.py` runs every check (37 per app) with
the edge cache enabled or disabled.

| Run | Laravel 12 | Laravel 13 |
| --- | --- | --- |
| cache ON | 34 PASS, 3 N/A (Swoole-only) | 33 PASS, 3 N/A, 1 FAIL* |
| cache OFF | 34 PASS, 3 N/A | 33 PASS, 3 N/A, 1 FAIL* |

\* `--max-requests recycling under load, new connection per request`: one
`ConnectionResetError` out of ~32 k requests, identical with the cache on and
off. Not a regression: the released 2.2.14 runtime (main) fails this check
3/3 in isolation, this branch passes it 3/3 in isolation (it counts only PHP
requests toward `--max-requests` and drains more cleanly); the residual
single reset shows up only under the full matrix load.

The static-files check (`If-None-Match` -> 304) failed with the cache on before
commit `57c3d328` (conditional requests were answered by the cache layer on a
miss instead of reaching PHP); it passes now in both modes.

## Cross-user isolation (anti-leak) battery

`edge_cache_isolation` in `packages/octane/readiness/matrix/matrix.py`.
48 concurrent users, each with its own identity (cookie, bearer token, query
string, real Laravel session), mixed with 25% anonymous callers on the same
URLs, random `Accept-Encoding` (gzip / br / identity / none), half on
keep-alive and half on fresh connections, 3 workers. Every response must
carry the caller's own identity.

| Mode | Requests per app | Labels (L12 run) | Leaks |
| --- | --- | --- | --- |
| Default (Cookie/Authorization bypass) | 5 760 | 3 585 BYPASS, 343 MISS, 1 832 HIT | 0 |
| Cookies + Authorization allowed, routes with `Vary: Cookie, Authorization`, 1 s TTL + SWR | 11 520 | 5 445 MISS, 6 038 HIT, 37 STALE | 0 |

Run 5 times in total (L12 and L13, cache ON and OFF matrix passes, plus a
standalone run): 0 leaks in ~86 k checked responses.

Negative control (proves the battery detects leaks): a route that marks
per-cookie content `public` without `Vary`, with cookies allowed (the
misconfiguration the docs warn against): 546 leaks detected in 640 requests.

## Soak with chaos (cache ON)

`soak.sh`, Laravel 12, 4 workers, `--max-requests=20000`, 8 MiB cache,
15 minutes of four concurrent wrk streams: cached Blade view (32 conns),
uncached view (8), cache-busting random query strings on the cached route (8,
forces constant eviction), session pages with a new cookie per request (4).
An `octane:reload` every 60 s (14 total) and a `kill -9` of a random worker
every 90 s (9 total).

| Stream | Requests | Req/s | p99 | Non-2xx |
| --- | --- | --- | --- | --- |
| Cached view (HIT) | 118 904 012 | 132 106 | 2.34 ms | 0 |
| Uncached view | 746 114 | 829 | 944 ms | 0 |
| Cache-busting (MISS + eviction) | 652 951 | 725 | 976 ms | 0 |
| Session pages (BYPASS) | 154 672 | 172 | 1.00 s | 0 |

- 41 socket read errors and 5 timeouts over ~120.5 M requests, all
  coinciding with the 9 `kill -9`s (connections in flight on the killed
  worker); no non-2xx response.
- Per-worker cache stayed at its bound throughout: ~2.0 MB / 1 024 entries
  with continuous LRU eviction under the cache-busting stream, back to empty
  after each reload or kill and refilled within one sample.
- Worker RSS/PSS could not be isolated in this run: other agents' PAM
  processes on the same host matched the sampler. Sampled totals showed no
  upward trend (1.56 GB at t=0, 1.77 GB at t=870 s with more processes
  matched), and the readiness check "Memory stability over 50k requests"
  passes with the cache on (PHP heap and worker RSS unchanged).

## Benchmark (cache OFF vs ON)

`wrk -t4 -c64 -d10s`, Laravel 12, 4 workers, server on CPUs 0-7, wrk on CPUs
16-19, best of 3 per run, median of 3 runs. The host was shared with other
agents' load tests (load average 15-37), so uncached numbers carry about
+-30 % noise.

| Route | Cache OFF req/s | Cache ON req/s | Cache ON p99 |
| --- | --- | --- | --- |
| `/_r/hello` (not cacheable) | 4 964 | 5 890 | 16.7 ms |
| `/_r/json` (not cacheable) | 4 999 | 6 203 | 16.1 ms |
| `/_r/view` (not cacheable Blade) | 4 961 | 5 277 | 18.8 ms |
| `/_r/view-cached` (Blade, `CacheResponse`) | 5 000 | 309 017 (HIT) | 2.9 ms |

No measurable overhead on routes that are not cached; a cached Blade page is
served from Rust at ~60x the PHP throughput (402 k req/s best run).
