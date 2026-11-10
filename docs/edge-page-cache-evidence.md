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
