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
