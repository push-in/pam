# PAM Process Manager

The PAM Process Manager keeps PHP and Laravel applications online after their
launching terminal exits. It builds on PAM's existing master/worker supervisor,
readiness gate, crash recovery, worker recycling, and generational reload.

```bash
pam up --name billing --workers 8
pam up --name critical --restart-delay-ms 250 --restart-backoff-max-ms 15000 \
  --max-unstable-restarts 10 --min-uptime-ms 30000
pam up --name production --env-file .env.production
pam up --name production --shutdown-timeout-ms 20000
pam up --name lean-api --workers 16 --php-extension iconv \
  --php-extension mbstring --php-extension pdo --php-extension pdo_mysql
pam up --name api --health-check-url http://127.0.0.1:8080/health \
  --health-check-interval-ms 5000 --health-check-timeout-ms 1000 \
  --health-check-start-period-ms 30000 --health-check-failures 3
pam ps
pam status billing
pam describe billing
pam logs billing --errors --lines 200
pam logs billing --both --follow
pam daemon start
pam daemon status
pam reload billing
pam restart billing
pam scale billing 12
pam monit
pam save
pam resurrect
pam startup --print
pam startup --install
pam config:check pam.toml
pam apply pam.toml
pam deploy billing /srv/billing/releases/2026-08-21
pam deploy:history billing --json
pam rollback billing
pam traffic:start billing-edge --listen 127.0.0.1:8080 --stable 127.0.0.1:8081 \
  --tls-cert /etc/pam/billing.pem --tls-key /etc/pam/billing.key
pam traffic:set billing-edge --candidate 127.0.0.1:8082 --weight-bps 500
pam traffic:evaluate billing-edge --min-candidate-requests 1000 \
  --max-candidate-error-bps 50 --json
pam traffic:status billing-edge --json
pam traffic:promote billing-edge
pam stop billing
pam delete billing
```

`pam up` starts `index.php` by default. Inside a Laravel project containing an
`artisan` file, it starts PAM Octane automatically. An explicit script selects
the raw Runtime path. Arguments after `--` belong to the application.
`--attach` keeps standard input, output, and error connected to the terminal.
Detached applications write separate output and error logs.

Detached applications automatically recover an unexpectedly exited master.
The delay doubles after each unstable restart, is capped by
`--restart-backoff-max-ms`, and resets only after `--min-uptime-ms` of stable
uptime. After `--max-unstable-restarts`, PAM opens a circuit and requires an
explicit `pam restart NAME` to retry. `--no-autorestart` disables this policy.
`status`, `describe`, and `ps --json` expose desired state, recovery state,
attempt counters, total automatic restarts, and the next retry deadline.
The reproducible `benchmarks/process-manager/run.sh` harness repeatedly kills
an isolated managed master and records raw recovery latency, success rate and
daemon RSS growth. Its suite-5 evidence manifest binds every artifact by
SHA-256 and detects later modification. The `Performance evidence` workflow
accepts manual suite ID `5`, builds the measured release binary on Ubuntu 24.04,
runs ten recovery rounds, verifies the manifest before upload, and retains the
clean-host artifact for 30 days. Its default gates require 100% recovery,
total p95 at most 200 ms, detection p95 at most 10 ms, effective backoff p95 at
most 20 ms, readiness p95 at most 150 ms, and daemon RSS growth at most 16 MiB.
A launch failure
keeps the workflow red but retains at most 64 KiB of CLI diagnostics and 1 MiB
of application stderr so clean-host failures remain actionable after cleanup.

The first hosted Ubuntu 24.04 suite-5 run is
[32469172846](https://github.com/push-in/pam/actions/runs/32469172846), bound to
commit `c644031`. All 10 masters recovered: p50 was 626 ms, p95 and maximum were
630 ms, and daemon RSS grew by 823296 bytes. Success, latency, and resource gate
codes were all `1`. The downloaded four-artifact bundle passed an independent
offline manifest verification after the workflow's own verification.

Suite `6` runs the same crash-recovery protocol against PAM and the exact
lockfile-pinned PM2 7.0.3 on one Ubuntu host: one PHP application instance, the
same ten `SIGKILL` rounds, 10 ms configured restart delay, 10 ms polling and a
10-second per-round deadline. It reports each system independently and uses
topology code `1` for PAM's master/worker replacement and `2` for PM2's directly
managed single process. Latency deltas are directional evidence, not a claim
that those topologies perform identical work; daemon RSS is explicitly marked
non-comparable. Both systems must recover every round, and the recursive suite-6
manifest binds the PAM report, both raw CSV/resource pairs, tool versions,
PM2 package integrity and shared parameters.
The isolated benchmark lock overrides PM2's vulnerable `js-yaml` 4.3.0 with the
compatible 4.3.1 security release, and CI rejects high-severity production-tree
advisories before running the comparison.
Evidence provenance defines `source.dirty` over tracked files, so the pinned
auxiliary `pam-native` checkout does not create a false dirty result; its exact
40-character commit is recorded separately as `tools.pam_native_commit`.

The [first clean hosted suite-6 run](https://github.com/push-in/pam/actions/runs/32472443364)
on Linux commit `44a0304` recovered 10/10 processes for both systems. PAM's
master/worker recovery measured p50 616 ms and p95/maximum 668 ms; PM2's direct
single-process recovery measured p50 112 ms and p95/maximum 122 ms, a directional
p95 delta of 546 ms. The evidence records `dirty=false`, PAM Native commit
`26a768c`, PM2 7.0.3 and its package integrity. All gate codes were `1`, and the
downloaded eight-artifact bundle passed independent offline verification. The
RSS values remain intentionally excluded from comparison because the daemon
topologies and responsibilities differ.

On Linux, `pamd` watches the exact registered master processes with `pidfd`.
One allocation-stable `poll()` set covers those descriptors and the private Unix
command socket, with a timeout aligned to the earliest restart deadline. It wakes
immediately for crashes and commands, while the 250 ms scan remains a compatibility
fallback when the running kernel cannot provide a descriptor. This avoids both
the former two-pass recovery delay and aggressive idle polling across large
application catalogs.

Each application status now exposes monotonic timestamps for last exit detection,
recovery start and recovery readiness. Suite 5 derives a separately bound
`recovery-phases.csv` and aggregates detection, effective backoff, master/worker
readiness and accounted time without changing the original recovery CSV contract.
A controlled local 10-round run measured total p50/p95 131/137 ms: detection
p95 1 ms, backoff p95 12 ms and readiness p95 72 ms. The comparison measured
PM2 p95 127 ms, leaving a 10 ms directional total gap.

The [hosted optimized suite-6 run](https://github.com/push-in/pam/actions/runs/32501406324)
confirmed the result on clean Linux commit `795a20c`: PAM recovered 10/10 with
p50 197 ms and p95/maximum 201 ms, down 70% from the previous hosted p95 of
668 ms. PM2 recovered 10/10 with p50 114 ms and p95/maximum 119 ms, leaving an
82 ms directional p95 gap. PAM daemon RSS grew 651264 bytes, but remains excluded
from cross-topology comparison. The stricter 300 ms PAM p95 gate and every other
gate passed with code `1`; the downloaded eight-artifact bundle passed independent
offline verification with `dirty=false` and the expected pinned tool identities.

The [hosted event-driven suite-6 run](https://github.com/push-in/pam/actions/runs/32504298849)
confirmed the phase model on clean Linux commit `d79b0ec`. PAM recovered 10/10
with p50 167 ms and p95/maximum 169 ms; PM2 recovered 10/10 with p50 138 ms and
p95/maximum 146 ms, leaving a 23 ms directional p95 gap. PAM phase p95 values
were 1 ms detection, 12 ms effective backoff, 92 ms readiness and 104 ms
accounted time. Its daemon RSS grew 819200 bytes. Total, detection, backoff,
readiness, success and resource gates all returned code `1`, as did all three
comparison gates. The downloaded nine-artifact bundle passed independent offline
verification with `dirty=false` and the pinned PAM, PAM Native, PHP and PM2
identities.

Suite `7` tests whether PAM's stronger master/worker recovery contract remains
bounded as the application grows from 1 to 4 and 16 workers. Each configuration
runs ten independent `SIGKILL` rounds and must pass 100% recovery, 10 ms
detection p95, 20 ms effective-backoff p95 and 16 MiB daemon RSS growth. The
total/readiness p95 budgets are 200/150 ms, 250/200 ms and 650/550 ms for 1, 4
and 16 workers respectively. The aggregate fails if any configuration is
missing, an unexpected one is present, round counts differ, or source, host,
binary and PAM Native provenance are not identical. A recursive SHA-256
manifest binds the aggregate report and every raw suite-5 artifact, and the
hosted workflow retains the complete Linux evidence for 30 days.
Gate failures remain workflow failures but no longer prevent the aggregate
report and verifiable manifest from being retained for diagnosis.

The [first passing hosted suite-7 run](https://github.com/push-in/pam/actions/runs/32507621605)
on clean Linux commit `5a74387` recovered all 30/30 masters. For 1, 4 and 16
workers, total p50/p95 was 165/169 ms, 223/241 ms and 539/580 ms; readiness
p50/p95 was 91/92 ms, 144/165 ms and 450/485 ms. Detection p95 remained 1 ms,
effective-backoff p95 was 11/12/13 ms, and daemon RSS growth was 851968,
770048 and 946176 bytes. Every per-configuration gate and both aggregate gates
returned code `1`. The downloaded bundle passed a second offline verification
of all 17 artifacts with `dirty=false` and the pinned source and tool identities.
The measured 16-worker readiness slope is now the primary optimization target;
the gate records the current clean-runner envelope rather than treating it as
the desired endpoint.

Suite `8` compares the compatible host-extension profile with an explicit,
fixture-minimal extension profile at 16 workers. It keeps the 650 ms total and
550 ms readiness budgets, a 25 ms detection envelope and a 250 ms
effective-backoff envelope because these phases include daemon scheduling delay
while sixteen workers terminate and start simultaneously.
The extension optimization itself remains guarded by equal successful round
counts and by requiring the isolated PHP-engine p95 to be no slower. The normal
suite-5 and suite-7 detection/backoff gates remain 10/20 ms.

The [first passing hosted suite-8 run](https://github.com/push-in/pam/actions/runs/32515500047)
on clean Linux commit `38e0781` recovered both profiles 10/10. Compatible versus
isolated p95 was 595/373 ms total, 493/252 ms readiness, 467/226 ms
spawn-to-ready and 416/60 ms PHP engine: improvements of 37.31%, 48.88% and
85.58% for the three gated comparison metrics. Detection p95 was 1 ms for both,
effective-backoff p95 was 16/15 ms and daemon RSS growth stayed below 1 MiB.
Every per-profile and aggregate gate returned code `1`; the downloaded bundle
independently verified all 14 bound artifacts with `dirty=false`.

`status`, `describe` and `ps --json` expose additive `workerStartup`
diagnostics for each ready generation: the time between spawning the first and
last worker, plus p95 and maximum spawn-to-ready latency across every worker.
The recovery harness records the same values per round in
`worker-startup.csv`, and suite 7 aggregates them per worker count. This keeps
the complete-generation readiness contract while separating process-launch
serialization from PHP/application bootstrap cost. The master checks worker
state every 5 ms during startup, down from 20 ms, without changing the 10 ms
accept-loop safety delay inside each worker.

The [first hosted run with startup diagnostics](https://github.com/push-in/pam/actions/runs/32510174077)
on clean Linux commit `1d2bc51` recovered all 30/30 masters and passed every
gate. For 1, 4 and 16 workers, total p50/p95 was 172/176 ms, 239/245 ms and
560/615 ms; readiness p50/p95 was 92/92 ms, 153/155 ms and 463/507 ms. Spawn
spread p50/p95 was 0/0 ms, 1/2 ms and 25/36 ms, while spawn-to-ready p50/p95
was 76/80 ms, 135/139 ms and 441/479 ms. Daemon RSS growth remained below
1 MiB in every configuration. The downloaded bundle independently verified all
20 artifacts against the clean source commit. Compared with the preceding
hosted run, total p95 changed by +7/+4/+35 ms and readiness p95 by 0/-10/+22 ms;
therefore this single paired run does not establish a polling-driven latency
improvement. It does establish that process-launch spread is a small part of
the 16-worker critical path: concurrent PHP/application bootstrap dominates.
The next optimization must target measured bootstrap work or safe reuse while
retaining worker isolation and complete-generation readiness.

### Explicit PHP extension isolation

PAM preserves the host PHP configuration by default. For applications whose
extension requirements are known, repeatable `--php-extension NAME` options
disable the host's global `conf.d` scan for workers and load only the selected
dynamic modules. `opcache` is loaded as a Zend extension; all other names use
PHP's normal extension loader. Names are deduplicated, limited to 64 entries
and 64 ASCII letters, digits, underscores or hyphens, so paths and injected INI
directives are rejected. The main `php.ini` remains active. The selection is
persisted by `pam up`, returned as `phpExtensions` by JSON inspection, accepted
by `pam start` and `pam octane:start`, and declared in `pam.toml`:

```toml
php_extensions = ["iconv", "mbstring", "pdo", "pdo_mysql", "opcache"]
```

This is an explicit compatibility/performance tradeoff, never an automatic
guess. `pam extensions [path] --no-dev` verifies Composer's official
`content-hash` algorithm locally, walks root and locked transitive `ext-*`
requirements, honors package `provide`/`replace`, and compares the compatible
PHP module inventory with the same main `php.ini` running without `conf.d`.
It reports integer-coded requirement origins, manifest/lock SHA-256 digests,
built-in, package-provided, selected and missing extensions, then prints the
exact repeatable arguments for operator review. `--json` exposes the versioned
schema-1 result. It refuses missing, stale, symlinked, malformed or oversized
Composer documents and never applies the profile automatically:

```bash
pam extensions . --no-dev
pam extensions . --no-dev --json
pam up public/index.php --name api --workers 16 --php-extension iconv
```

Composer's official [`validate`](https://getcomposer.org/doc/03-cli.md#validate)
contract likewise checks whether the lock is current, and its
[`Locker::getContentHash`](https://github.com/composer/composer/blob/main/src/Composer/Package/Locker.php#L84-L112)
defines the exact relevant keys and canonical hash PAM reproduces. Still run
`pam composer check-platform-reqs` and application tests against the production
build before deployment. Omitting extension arguments retains the complete
configured extension set.
Official PHP guidance says CLI preloading is generally pointless unless the
process persists and that preloading is unavailable on Windows, so PAM does
not silently substitute `opcache.preload` for extension isolation.

Local suite `8` used the fixture's sole dynamic requirement, `iconv`: across
ten crash-recovery rounds per profile it reduced total p95 from 230 to 171 ms,
readiness p95 from 142 to 83 ms and PHP-engine p95 from 77 to 10 ms. That is a
25.65%, 41.55% and 87.01% directional improvement respectively. Both profiles
recovered 10/10, every gate passed, and the recursive manifest verified all 14
artifacts. Hosted evidence is required before treating the delta as portable.

The Composer-derived follow-up removed the hardcoded isolated list from suite
`8`. On a clean local process table, `pam extensions --no-dev` verified the
fixture lock and selected `iconv`; both profiles again recovered 10/10. Total,
readiness and PHP-engine p95 changed from 184/122/59 ms to 148/81/10 ms,
improvements of 19.57%, 33.61% and 83.05%. All gates passed, the recursive
manifest verified 15 artifacts including the exact profile decision, and no
fixture process remained after the campaign.

Linux workers now install `PR_SET_PDEATHSIG` before `exec` and verify that the
master did not exit during the fork/exec window. An unexpected master
`SIGKILL` therefore kills its workers at the kernel boundary instead of leaking
detached PHP processes. A cluster integration test captures both worker PIDs,
kills the master and requires both `/proc` entries to disappear within five
seconds. Graceful shutdown remains unchanged and still drains workers first.

`--env-file FILE` supplies per-application environment without copying secret
values into manager records, JSON output, logs, or `pam.toml`. PAM reads the
file again on every launch and restart, so an explicit `pam restart NAME`
activates rotated values. The format is bounded `KEY=VALUE` text with optional
single or double quotes and an optional `export ` prefix; interpolation and
shell execution never occur. The file must be a non-symlink regular UTF-8 file
owned by the current user, mode `0600` or stricter, at most 64 KiB and 256
variables. Manager state-directory overrides are reserved and rejected.

`--health-check-url` adds active liveness supervision for a master that remains
present but no longer serves healthy responses. PAM accepts only an explicit
loopback IP and port over HTTP, never follows redirects, never resolves DNS,
bounds the response prefix to 8 KiB, and treats only status `200`–`299` as
healthy. Probes run outside the daemon request thread with at most 64 in flight;
stale results are discarded by PID and configuration identity. After the
configured consecutive-failure threshold, PAM records the unhealthy event,
kills the stuck master, and delegates restart/backoff/circuit behavior to the
same recovery state machine. Health checks require automatic restart.
`--health-check-start-period-ms` defers liveness probes for 0–3600000 ms after
each master start. It defaults to zero for compatibility and uses the new
master's recorded start time after every manual or automatic restart. This
prevents a slow but valid warmup from consuming the failure threshold; status
JSON exposes the effective value as `healthCheck.startPeriodMillis`.

`--shutdown-timeout-ms` controls how long PAM waits after `SIGTERM` during
`stop`, `restart`, deploy activation, and rollback. The accepted range is
100–300000 ms and the default is 20000 ms. If the master is still alive, PAM
escalates to `SIGKILL`, verifies termination for up to five seconds, and reports
the forced shutdown in human and JSON output. This keeps automation bounded
while preserving graceful draining under normal operation.

Log files rotate before launch or restart when they reach 10 MiB and retain
five generations by default. `pam up --log-max-bytes N --log-retain N` stores
per-application limits. `pam logs NAME --follow` streams appended bytes and
continues across rotation; `--errors` selects stderr and `--both` follows both.
For bounded operational queries, combine `--query TEXT`, `--lines N`,
`--include-rotated`, `--both` and `--json`. The versioned result uses stream
codes stdout `1` and stderr `2`, includes only the rotation index and line, and
never exposes manager filesystem paths. Queries retain at most 10,000 matching
lines from bounded 8 MiB windows per file, accept lossy non-UTF-8 content, and
reject empty, oversized or control-bearing filters. Structured query options
cannot be combined with the unbounded interactive `--follow` mode.

## Lifecycle semantics

- `reload` starts a new generation, waits for readiness, activates it, and then
  drains the old generation. Failed readiness keeps the healthy generation.
- `restart` resets an open recovery circuit, gracefully stops the master, and executes its recorded command again.
  It has a downtime window and receives a new PID.
- `stop` persists stopped intent before signaling, retains registration and
  logs, and is never undone by automatic recovery.
- `delete` accepts only stopped applications and preserves their logs.
- `scale` persists a new worker target and performs a readiness-gated restart.
- `save` records online/stopped intent in a bounded schema; `resurrect` starts
  only applications saved as online and skips healthy processes.
- `monit --json` emits a one-shot automation-friendly health/capacity snapshot.

All lifecycle commands accept `--json`. Public kind and state values are
sequential integer enums: Runtime kind `1`, Laravel Octane kind `2`, online
state `1`, and stopped state `2`. Recovery states are healthy `1`, backoff `2`,
stabilizing `3`, circuit open `4`, and disabled `5`.
Health states are disabled `1`, healthy `2`, failing `3`, and unhealthy `4`.
The bounded pre-liveness startup period is starting `5`.

## Linux state and security

The default root follows the XDG base-directory convention:

```text
$XDG_STATE_HOME/pam/
├── applications/   bounded schema-2 registrations
├── runtime/        master state and PID fingerprints
└── logs/           stdout and stderr streams
```

`pamd` uses `$XDG_RUNTIME_DIR/pam/pamd.sock` (with a private state-directory
fallback), mode `0600` inside a `0700` directory. The server accepts only the
same effective UID, verified from Linux `SO_PEERCRED`, and bounds each request
to 16 KiB. Start, inspect, and stop it with `pam daemon start|status|stop`.
When `pamd` starts, it restores a saved process list if present.

Non-interactive manager commands auto-start `pamd` and execute through its
serialized, allowlisted RPC authority. Responses preserve stdout, stderr, and
exit status and are bounded to 2 MiB. Malformed clients receive an error without
terminating the daemon. Interactive `pam up --attach` and `pam logs --follow`
remain attached directly to the invoking terminal by design.

`pam startup --print` generates a hardened foreground systemd user unit.
`pam startup --install` writes it atomically to
`~/.config/systemd/user/pamd.service` without changing unrelated configuration.
Enable it explicitly with `systemctl --user enable --now pamd.service`. Run
`pam save` first to select which applications return after login or reboot.

## Declarative multi-service configuration

`pam apply` reconciles a strict, versioned `pam.toml`. Missing applications are
created, stopped applications restart, worker drift is scaled, converged
applications stay untouched, and `autostart = false` stops an existing process.

```toml
schema_version = 1

[applications.api]
kind_code = 1
script = "public/index.php"
workers = 4
cwd = "."
arguments = ["--port=8080"]
php_extensions = ["iconv", "mbstring", "pdo", "pdo_mysql", "opcache"]
memory_warning_bytes = 536870912
task_warning_count = 64
memory_max_bytes = 805306368
task_max_count = 96
env_file = ".env.production"
shutdown_timeout_millis = 20000
health_check_url = "http://127.0.0.1:8080/health"
health_check_interval_millis = 5000
health_check_timeout_millis = 1000
health_check_start_period_millis = 30000
health_check_failure_threshold = 3
auto_restart = true
restart_delay_millis = 250
restart_backoff_max_millis = 15000
max_unstable_restarts = 10
min_uptime_millis = 30000

[applications.web]
kind_code = 2
workers = 8
cwd = "apps/web"
autostart = true
```

Kind `1` is raw PAM Runtime; kind `2` is Laravel Octane and requires `artisan`
in its working directory. Working directories must remain beneath the
configuration directory, worker counts are bounded to 1–256, unknown fields
fail validation, and configuration files are limited to regular non-symlink
files of at most 1 MiB. Relative environment-file paths resolve from the
`pam.toml` directory and changes restart a running application so the new
environment is effective. Use `pam config:check --json` in CI and `pam apply
--json` for a stable action-coded reconciliation report.

`memory_warning_bytes` and `task_warning_count` are optional positive alert
thresholds. On Linux, `status`, `describe` and `monit` inspect the complete
descendant process tree under the recorded supervisor and report aggregate RSS,
threads and process count. Alert state codes are healthy `1`, memory `2`, tasks
`3`, both `4`, unavailable `5`; applying threshold-only changes uses reconcile
action `6` without restarting a healthy application. These are observation
thresholds, not cgroup enforcement limits.

`memory_max_bytes` and `task_max_count` are opt-in hard limits. PAM launches the
application in a unique collected systemd user scope with `MemoryMax` and
`TasksMax`; if systemd cannot create the scope, PAM fails readiness instead of
silently running without the requested policy. The resource snapshot reads
`memory.max` and `pids.max` from the master process's actual cgroup. Enforcement
codes are verified `1`, not requested `2`, unverified/mismatched `3`. A hard-limit
change restarts the application and returns reconcile action `7`; warning-only
changes remain action `6`. Warnings must not exceed their corresponding maximum.

## Bounded resource history

The private per-user daemon records one resource sample per managed application
at startup and every 60 seconds. `pam monit:history [NAME] [--limit N]` reads the
latest entries; add `--json` for schema-versioned automation or `--record` to
capture an immediate incident sample. Limits range from 1 to 120.

Every application has an independent owner-only history capped at 120 entries.
Entries contain only observation time, sequential integer process/alert states,
worker count, aggregate RSS, and task count. Commands, paths, environment,
network details, and logs are excluded. `pam delete` removes the corresponding
history, while stopped applications remain explicitly sampled with state code
`2` and unavailable alert code `5` until deletion.

`pam dashboard` summarizes the bounded window as exact sample count, peak RSS,
and textual RSS direction. This supplements the current snapshot; the JSON
history remains the authoritative lossless local evidence.

## Authenticated live dashboard

`pam dashboard:start --token-file TOKEN [--listen 127.0.0.1:PORT]` starts a
detached read-only HTTP view. `dashboard:status` reports its sequential state
code (`1` online, `2` stopped), PID, loopback listener and start time;
`dashboard:stop` terminates it and removes both runtime state and the private
credential digest. The default listener is `127.0.0.1:9615`, and every other
loopback port is accepted except zero. PAM rejects all non-loopback addresses.
The runtime state binds the PID to its Linux process-start ticks, so stale state
cannot make status trust—or stop signal—an unrelated process after PID reuse.

The token file follows the hardened control-plane credential contract and must
also be owner-only on Linux. Only a SHA-256 digest is persisted. Both browser
Basic authentication (`pam:TOKEN`) and Bearer authentication use constant-time
digest comparison. Requests are limited to 16 KiB of headers and two seconds of
read/write time; only authenticated `GET /` and `GET /health` exist. Responses
are `no-store`, deny framing/referrers/sniffing, and carry a script-free CSP.

The live page deliberately has no timer or JavaScript. “Refresh now” performs a
new authenticated GET and leaves refresh under explicit user control, avoiding
unexpected screen-reader announcements or keyboard-focus resets. It shares the
same 2 MiB rendering bound and privacy exclusions as static snapshots.

## Transactional releases

`pam deploy NAME RELEASE_DIRECTORY` activates an already-built release without
copying it into PAM state. PAM stops the previous generation, changes the
recorded working directory atomically, starts the candidate, and confirms its
normal readiness gate. If readiness fails, PAM restores the previous record and
reactivates the last release; a failed recovery is reported separately.

Deploying the active canonical directory is idempotent. Successful transitions
are recorded in a private, schema-versioned history capped at 50 entries.
`pam rollback NAME [--steps N]` selects an earlier distinct healthy release and
uses the same transactional activation. Release arguments and script layout
remain identical across releases. Symlink release roots are rejected; release
artifacts remain operator-owned and are never silently deleted by PAM.

JSON action codes are sequential: activated `1`, rolled back `2`, unchanged
`3`. History event kinds are baseline `1`, deploy `2`, rollback `3`.

## Progressive traffic delivery

The version ingress runs stable and candidate upstreams side by side. Candidate
weight uses integer basis points (`0`–`10000`) and updates atomically through a
monotonically increasing generation. `traffic:abort` immediately restores
stable-only routing; `traffic:promote` makes the current candidate stable and
removes the candidate slot.

Routing affinity hashes the `pam_affinity` cookie when present, otherwise
the client IP and request path. PAM overwrites forwarding headers and removes an
incoming `x-pam-release` before proxying; the trusted response receives
`x-pam-release: stable|candidate`. Request bodies and responses stream through
the existing Hyper transport and WebSocket upgrades remain bidirectional.

`traffic:status --json` reports separate request, 5xx/upstream-error, and total
latency-microsecond counters for stable and candidate. Metrics are private,
bounded, periodically persisted, and contain no URLs, cookies or client
identifiers. Use those counters as rollout evidence before an explicit promote.

Every weighted candidate starts rollout phase evaluating `2` with a bounded
deadline (300 seconds by default, configurable with `--deadline-seconds`). Metrics
carry the active generation and reset atomically when routing changes; late
responses from an older generation cannot contaminate a gate. `traffic:evaluate`
requires a minimum candidate sample and maximum error rate in basis points. Its
persisted decision codes are pending `1`, promoted `2`, error-aborted `3`, and
deadline-aborted `4`; terminal phase codes are promoted `3` and aborted `4`.
Pending returns exit code 1 without discarding accumulated evidence. Promotion
and abort remain explicit commands for operator override.

The ingress accepts HTTPS with `--tls-cert FILE --tls-key FILE` while continuing
to proxy over HTTP to explicit upstream `IP:port` addresses. PAM requires both
PEM files, resolves them before detaching, rejects symlinks, empty or oversized
files, and never includes their paths in status output. Certificate rotation
currently requires restarting the traffic ingress. PAM rejects self-referential
listeners, invalid weights and weight without a candidate.

Without `XDG_STATE_HOME`, PAM uses `$HOME/.local/state/pam`. Directories are
mode `0700`; records and logs are mode `0600`. Application names are limited to
64 ASCII letters, digits, dots, hyphens, or underscores. PAM rejects symlink
state directories, records, and log targets, bounds records to 1 MiB, and
refuses inventories above 1,024 applications.

The master state contains the Linux `/proc` process-start fingerprint. PAM
checks PID liveness and that fingerprint before sending a signal, avoiding
control of an unrelated process after PID reuse.

`PAM_MANAGER_STATE_DIR` overrides the root for isolated tests. Production
operators should prefer the XDG location and must not share it between users.

## Delivery boundary

This contract supplies detached application management, durable rotating logs,
and the authenticated per-user `pamd` control-plane foundation. Compatible
additions include moving every lifecycle mutation through the daemon,
multi-service `pam.toml`, deployment history, and rollback. Existing Runtime,
Native, Desktop, and Octane commands remain independent while that authority is
expanded.
