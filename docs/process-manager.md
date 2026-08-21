# PAM Process Manager

The PAM Process Manager keeps PHP and Laravel applications online after their
launching terminal exits. It builds on PAM's existing master/worker supervisor,
readiness gate, crash recovery, worker recycling, and generational reload.

```bash
pam up --name billing --workers 8
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
pam traffic:start billing-edge --listen 127.0.0.1:8080 --stable 127.0.0.1:8081
pam traffic:set billing-edge --candidate 127.0.0.1:8082 --weight-bps 500
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

Log files rotate before launch or restart when they reach 10 MiB and retain
five generations by default. `pam up --log-max-bytes N --log-retain N` stores
per-application limits. `pam logs NAME --follow` streams appended bytes and
continues across rotation; `--errors` selects stderr and `--both` follows both.

## Lifecycle semantics

- `reload` starts a new generation, waits for readiness, activates it, and then
  drains the old generation. Failed readiness keeps the healthy generation.
- `restart` gracefully stops the master and executes its recorded command again.
  It has a downtime window and receives a new PID.
- `stop` retains registration and logs.
- `delete` accepts only stopped applications and preserves their logs.
- `scale` persists a new worker target and performs a readiness-gated restart.
- `save` records online/stopped intent in a bounded schema; `resurrect` starts
  only applications saved as online and skips healthy processes.
- `monit --json` emits a one-shot automation-friendly health/capacity snapshot.

All lifecycle commands accept `--json`. Public kind and state values are
sequential integer enums: Runtime kind `1`, Laravel Octane kind `2`, online
state `1`, and stopped state `2`.

## Linux state and security

The default root follows the XDG base-directory convention:

```text
$XDG_STATE_HOME/pam/
├── applications/   bounded schema-1 registrations
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
files of at most 1 MiB. Use `pam config:check --json` in CI and `pam apply
--json` for a stable action-coded reconciliation report.

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

The ingress currently speaks HTTP to explicit upstream `IP:port` addresses. TLS
should terminate at the existing PAM/edge TLS boundary until certificate-backed
version-ingress listeners are shipped. PAM rejects self-referential listeners,
invalid weights and weight without a candidate.

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
