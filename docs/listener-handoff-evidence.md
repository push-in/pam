# Master-owned listeners: test evidence

Branch `fix/balance-drain-handoff`. Machine: Intel Core Ultra 9 185H, Linux
7.0.11. Build: release profile like the Linux release job (Ubuntu 22.04, PHP
8.5 headers) linked against the `libphp.so` 8.5.12 of the PAM 2.2.14 layout.
Baseline: the released PAM 2.2.14 (= origin/main 83dfb5fb). App: Laravel 12 +
Octane from the readiness suite. Checks: `matrix.py --only max_requests`
(4 workers, `--max-requests=500`, 16 clients for 12 s with a new connection
per request, then 12 s keep-alive) and `matrix.py --only zero_loss`
(`wrk -c64` keep-alive for 40 s, `--max-requests=2000`, a reload every 4 s).
`tcp_migrate_req=1` runs use a private network namespace
(`unshare -rn`, sysctl set inside it).

## Root cause

Every worker bound its own `SO_REUSEPORT` listener and closed it when it was
recycled or reloaded. Closing a listener resets the handshakes the kernel had
queued on it. `tcp_migrate_req=1` moves them to a sibling listener, but not
when that sibling is closing at the same time, which is common: balanced
workers reach `--max-requests` close together, and a reload closes every
listener of a generation. The failures were all `ConnectionResetError` during
`connect()` (a client logging the phase of each failure showed `phase=connect`
for every one). Keep-alive clients hit the same window when a draining worker
answers `Connection: close` and they reconnect, which is the keep-alive write
error reported in PR #155 (swoole-parity RUNS.txt, item 10).

The connection balancer only made it more frequent (it equalizes request
counts, so recycles line up); it never dropped a handed-off connection:
`PAM_BALANCE=0` hid the problem by spreading recycles, not by fixing it.

## Fix

The master keeps a duplicate of every worker listener (`src/listener_registry.rs`,
a `SOCK_SEQPACKET` socket in the master's 0700 runtime directory, descriptors
passed with `SCM_RIGHTS`). A recycled worker's replacement, a restarted worker
and the same worker id of the next generation adopt the existing socket for the
same address instead of binding a new one; a draining worker only drops its own
copy. A socket with queued connections is never closed while the cluster runs.
The master closes a listener when the activated generation no longer listens on
its address, when a failed generation created it, when its worker crashed and
waits for a backoff restart, and at shutdown.

## Results

| check | `tcp_migrate_req` | 2.2.14 (main) | this branch |
|---|---|---|---|
| recycling, new connection per request | 0 | 16 failed / 569,385 (7 of 10 runs) | **0 / 518,202** (10 runs) |
| recycling, new connection per request | 1 | 3 failed / 519,479 (2 of 10 runs) | **0 / 587,807** (10 runs) |
| recycling, keep-alive | 0 | 1 failed / 812,496 (1 of 10 runs) | **0 / 777,734** (10 runs) |
| recycling, keep-alive | 1 | 0 / 745,550 (10 runs) | **0 / 842,491** (10 runs) |
| zero-loss: keep-alive + recycling + reload every 4 s | 0 | 1 failed / 2,106,464 (1 of 10 runs) | **0 / 2,657,555** (10 runs) |
| zero-loss: keep-alive + recycling + reload every 4 s | 1 | not run | **0 / 2,440,512** (10 runs) |

This branch: **60 of 60 runs passed, 7,824,301 requests, 0 failures.**

Fixture reproduction (`tests/fixtures/server.php`, 4 workers,
`--max-requests 500`, 16 clients, new connection per request, 60 s):
main 2 and 4 resets in 466k and 597k requests with `tcp_migrate_req=1`;
this branch 0 in 590,887 (`=0`) and 0 in 602,862 (`=1`).

Tests: `listener_registry` unit tests (adoption keeps the queued connection,
release by generation/failed generation/worker, malformed requests and
non-listener descriptors rejected) and
`recycled_workers_and_reloads_reuse_the_listeners_the_master_keeps` (the
master holds both listeners; after recycling and a reload the workers hold the
same two sockets). `recycles_workers_make_before_break_without_dropping_requests`
now keeps sending until replacements served: with a fixed 600 requests a fast
machine finished before the first replacement was ready (it failed the same
way on unmodified main).
