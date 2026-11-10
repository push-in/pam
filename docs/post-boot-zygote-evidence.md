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
| Octane's own suite through the PAM client | pass | pass |
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
