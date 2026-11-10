# Post-boot zygote: test evidence

Feature: `PAM_PREFORK_BOOT=app` (default for `artisan pam:octane`), PR #153.
Machine: Intel Core Ultra 9 185H (22 threads), 30 GiB RAM, Linux 7.0.11.
Build: release profile built like the Linux release job (Ubuntu 22.04, PHP 8.5
headers, linked against the `libphp.so` 8.5.12 of the installed PAM 2.2.14
layout), installed into a copy of that layout. App: Laravel 12 + Octane 2.24 +
`pushinbr/pam-octane` from this branch, MySQL 8.4 and Redis 7 (predis) in
disposable containers. Harness: `packages/octane/readiness/zygote/`.

Every run is listed with its command and result. Nothing was retried away.

## 1. Unit and integration tests

| Command | Result |
|---|---|
| `cargo test --bin pam -- prefork cluster::tests::boots` | 7 passed |
| `cargo test --test cluster` (default mode) | 15 passed |
| `PAM_PREFORK_BOOT=app cargo test --test cluster` (whole suite forked from a booted app) | 15 passed |
| `cargo clippy --all-targets -- -D warnings`, `cargo fmt --check` | clean |
| `packages/octane`: `phpunit`, `phpstan analyse` | 22 tests OK, no errors |

Mutation checks (each guard removed, the isolation test must fail): without
`pam_php_after_fork()` the test fails with "workers drew the same mt"; without
`reopen_regular_files()` it fails with "a shared file offset"; both pass again
with the code restored.

## 2. End-to-end isolation and numbers (`zygote_e2e.py`)

`zygote_e2e.py --pam pam-run --app laravel --mode {app,engine} --workers {8,16} --reloads 3`,
with the probe provider opening MySQL, Redis, a log file and a raw Redis
socket while booting.

Post-boot (app) mode, 8 and 16 workers: **0 failures**. Every worker has its
own MySQL `CONNECTION_ID()` and Redis `CLIENT ID`; MySQL `processlist` shows
exactly one connection per worker and none for the zygote; the raw boot
socket is closed (`CLIENT LIST` has no `zygote-raw`) and every worker's write
to it fails; `mt_rand`, `rand`, `uniqid`, `Str::random`, `random_int`,
`Str::uuid` differ across workers; 64 concurrent clients logged in as 64
users, 20 requests each: no session or auth crossed users, per-session hit
counters stayed exact; a reload boots a new zygote (new boot PID).

| | post-boot 8 | pre-boot 8 | post-boot 16 | pre-boot 16 |
|---|---:|---:|---:|---:|
| PSS total (workers + zygote) | 158 MB | 225 MB | 199 MB | 317 MB |
| PSS per worker | 15.2 MB | 25.5 MB | 10.4 MB | 18.6 MB |
| worker spawn -> ready (p50 / max) | 4 / 23 ms | 31 / 375 ms | 2 / 5 ms | 452 / 460 ms |
| cold start (all ready) | 351 ms | 432 ms | 352 ms | 515 ms |
| reload (`SIGHUP` -> generation ready) | 342-524 ms | 423-564 ms | 363-504 ms | 524-646 ms |

Pre-boot workers each also keep an idle MySQL connection of the console
application (2 per worker); post-boot closes it once in the zygote.

## 3. Readiness suite, feature ON and OFF

`readiness/run-local.sh suite matrix ecosystem` with `PAM_PREFORK_BOOT=app`
(ON) and `PAM_PREFORK_BOOT=engine` (OFF), release build.

| | ON | OFF |
|---|---|---|
| Octane 2.24 suite on PAM's PHP and through the PAM client | 182 pass, 0 fail, 43 skip (112 exchanges, 0 mismatches) | same |
| Laravel 12 + 13 matrix (35 checks each) | 69/70 | 68/70 |
| Ecosystem app (Livewire, Inertia, Filament, Horizon, Reverb), 17 steps | 17/17 | 17/17 |
| Server logs | `post-boot zygote ready in 247-263 ms` on every start/reload | pre-boot |

The only failing check, in both modes, is "`--max-requests` recycling under
load, new connection per request": 2 (ON) and 1 + 3 (OFF)
`ConnectionResetError` out of 57k-74k requests. It is pre-existing and
independent of this feature. Root cause isolation (`recycle-race.sh`, same
check repeated, Laravel 12):

| configuration | runs | resets |
|---|---|---|
| ON, `tcp_migrate_req=0` | 4 | 1, 4, 0, 0 |
| OFF, `tcp_migrate_req=0` | 4 | 0, 0, 0, 0 |
| ON, `tcp_migrate_req=1` (netns) | 4 | 0, 0, 0, 1 |
| OFF, `tcp_migrate_req=1` (netns) | 4 | 1, 0, 0, 0 |
| ON, `tcp_migrate_req=1`, `PAM_BALANCE=0` | 10 (430k requests) | **0** |

With migration on, resets remain only while the least-connections balancer
hands sockets over (`SCM_RIGHTS`) during a recycle; with the balancer off
they disappear. This is a transport issue outside the zygote and is reported
separately. Recycling itself is much cheaper with post-boot: the same 12 s
window served 44k-56k requests (ON) against 20k-29k (OFF).

## 4. 30-minute soak (reloads + recycling + isolation under load)

`SOAK_TMP=/tmp/pz-soak soak.py --mode app --minutes 30 --workers 8 --max-requests 2000 --reload-every 60 --sessions-every 20`
(`wrk -t4 -c64` over `/z/info` (MySQL + Redis), `/up`, `/z/slow?ms=3`; a session
isolation check every 20 s: 16 users x 10 requests plus per-worker
connection/RNG uniqueness; connection balancing on.)

Result: **4,205,186 requests, 0 non-2xx, 0 socket errors**, 2,336 rps, p50
23.7 ms, p99 127 ms; 29 reloads; 86 isolation checks, 0 failures; final state
healthy, 8 ready, generation 30. Total PSS flat at 101-145 MB (one 177 MB
sample during a reload overlap); master fds 12-14; master children 1-2
(zygotes of the old and new generation during a reload).

Reload p50 673 ms. Two reloads took 10.5 s and 15.9 s: the server log shows
the zygote's Laravel boot itself took 5.0 s and 14.7 s at those moments
(`post-boot zygote ready in 14659 ms`), while the workstation was under
memory pressure from other jobs (the master's RSS was reclaimed to 2.4 MB at
the same sample); the old generation kept serving and no request failed.

An earlier attempt of this soak was invalid and is not counted: the probe's
`lcg_value()` deprecation warnings filled the disk with `laravel.log`, Redis
refused writes (`MISCONF`) and the session check got 500s. The probe no
longer calls `lcg_value()` and the deprecation channel is off.

## 5. Repeated reload (1,647 consecutive reloads under load)

`soak.py --mode app --minutes 18 --workers 4 --max-requests 100000 --reload-every 0.05 --sessions-every 30 --wrk-connections 16`
(a new `SIGHUP` 50 ms after each generation became ready, i.e. continuous
reloading, with traffic and isolation checks running).

Result: **1,647 reloads, 1,290,046 requests, 0 non-2xx, 0 socket errors**;
35 isolation checks (sessions, auth, per-worker MySQL/Redis connections,
PRNG), 0 failures; final state healthy at generation 1,648. Master fds
constant at 14, master RSS flat (14.6 MB, later reclaimed to 5 MB by the
kernel), master children constant at 2 at every sample (the retiring and the
new zygote): no zygote, worker or descriptor is leaked per reload. Reload p50
594 ms. p50 latency 10.9 ms; p99 2.1 s is the cost of reloading without pause
(requests wait while generations swap back to back). One reload took 12.1 s
during the same workstation memory-pressure event seen in section 4.

## 6. Memory over 1.4M requests (leak check)

`soak.py --mode app --minutes 10 --workers 8 --max-requests 1000000000 --path /z/info --sample-every 20`
(no recycling, no reloads; every request queries MySQL and Redis).

Result: 1,435,474 requests, 0 non-2xx, 2,392 rps, p50 26 ms, p99 62 ms.
Per-worker memory from t = 60 s to the end (seven workers that lived the
whole run): RSS +72 to +248 KB per worker (511 -> 512 MB in total), PSS +0.5
to +0.7 MB per worker (93 -> 97 MB in total; PSS rises slightly as pages
written by a worker stop being shared with the zygote). Flat: no leak.

The eighth worker was killed with `SIGKILL` at t = 340 s, which cut 8 `wrk`
connections (8 read errors). It was not PAM: no watchdog or limit line was
logged, and the kernel OOM counters of this user's slice show the cause
(`/sys/fs/cgroup/user.slice/user-1000.slice/memory.events`: `high 74376`,
`oom_kill 9`; the slice has `memory.high` 24 GiB / `memory.max` 28 GiB shared
by every job on the workstation). The master replaced the worker from the
zygote and the run continued; the same OOM events also killed this test
session's shell twice.

## 7. Chaos (kill -9, OOM, memory_limit fatals, cut clients, reloads)

`soak.py --minutes 10 --workers 4 --max-requests 2000 --kill-every 15 --hog-every 60 --scope-memory 1500M --disconnect-every 3 --reload-every 45 --sessions-every 20 --wrk-connections 32`,
the cluster inside a `systemd-run --user --scope -p MemoryMax=1500M` so the
hog request (`/z/hog`, unbounded PHP memory) is OOM-killed by the kernel;
each hog is followed by a PHP `memory_limit` fatal (`/z/oom`); every 3 s a
client cuts a request mid-flight, one sends headers byte by byte and one sends
half a body then disconnects.

### Bug found and fixed: the OOM killer chose the zygote

First run (`chaos-app`): 546k requests with **184,786 `wrk` write errors**,
against 5 in pre-boot mode (`chaos-engine`, same command). A watcher sampling
the zygote PIDs and the listening sockets every 200 ms (`watch.sh`) showed
the cause: a zygote died outside any reload, its workers died with it
(`PR_SET_PDEATHSIG`), the port had 0-1 listeners for about 0.8 s and `wrk`
spun on refused connections until exec'd workers booted. The post-boot
zygote holds the booted application, so it is the largest idle process of
the scope and the OOM killer picked it (kernel `oom_kill` counter moved with
it). Fix (9a844a22): every forked worker raises its own `oom_score_adj` by 300
(no privilege needed), so the kernel kills the worker that is growing; the
master replaces it from the zygote in milliseconds. Covered by a unit test
and by the post-boot integration test (worker adj = zygote adj + 300).

### After the fix (`chaos-app2`, same command)

| | post-boot (after fix) | pre-boot |
|---|---:|---:|
| requests | 927,575 | 720,139 |
| rps | 1,546 | 1,200 |
| zygote deaths (watcher) | 0 | - |
| `wrk` write / read errors | 13 / 444 | 5 / 455 |
| non-2xx | 117 | 111 |
| kills / OOM kills / fatals / cut clients | 39 / 9 / 9 / 538 | 39 / 9 / 9 / 537 |
| reloads (p50 / max) | 13 (444 / 649 ms) | 13 (789 / 1,247 ms) |
| final | healthy, 4 ready | healthy, 4 ready |
| session isolation checks | 28, no leak | 26, no leak |

Every remaining error is a request that was inside a worker when it was
killed (`read` errors, the byte-by-byte probe connection) or queued behind a
`memory_limit` fatal (`non-2xx`: requests already accepted by that worker are
answered 500 while it is recycled), in the same proportion in both modes;
none is caused by forking from a booted application. Master fds stayed at
12 and master RSS at 16.3 MB.

## 8. `--max-requests` recycling cost

`soak.py --minutes 1 --workers 8 --path /up --wrk-connections 64 --max-requests {1000000000,200}`
(single 60 s runs, `wrk -t4 -c64`, Laravel's `/up`):

| | no recycling | recycling every 200 requests |
|---|---|---|
| post-boot | 11,897 rps, p50 5.03 ms, p99 7.85 ms | 14,249 rps, p50 4.26 ms, p99 11.2 ms |
| pre-boot | 11,508 rps, p50 5.14 ms, p99 8.40 ms | 10,749 rps, p50 5.75 ms, p99 10.0 ms |

With post-boot, recycling a worker every 200 requests costs no throughput
(a replacement is ready in 1-6 ms); pre-boot loses 7 %. 2 (post-boot) and 6
(pre-boot) `wrk` socket errors with recycling are the pre-existing
reconnect race of section 3.

## 9. Final binary

The final release build (with the OOM fix) passes `zygote_e2e.py` with 0
failures: 8 workers 152.5 MB PSS (14.9 MB per worker), spawn-to-ready p50
3 ms (max 6); 16 workers 193.0 MB PSS (10.2 MB per worker), p50 2 ms (max
4); reloads 362-584 ms.

The readiness suite was run again ON with the final binary
(`readiness.sh on suite matrix ecosystem`): Octane suite 182 pass / 0 fail /
43 skip (112 exchanges, 0 mismatches), ecosystem 17/17, matrix 69/70. The
one failure is the same pre-existing recycling reset of section 3, this time
in the keep-alive variant (1 of 96,369 requests).

## 10. CI

All PR checks green on 9a844a22 (OOM fix), including the cluster suite run
twice (default and `PAM_PREFORK_BOOT=app`) and the readiness workflow. The
only red check on the branch (aea0200a, "Laravel 13.0") exited with code 100
in its package-install step (apt) and passed on the next commits.

## Open item (outside this feature)

Resets during `--max-requests` recycling: 1-4 per ~50-100k requests in both
modes, with and without `tcp_migrate_req`, gone with `PAM_BALANCE=0`
(section 3). The connection balancer's hand-off during a worker drain needs
its own fix.
