# PAM Laravel production platform

`pushinbr/pam-laravel` is the operational layer for Laravel 12 and 13 on PAM.
It keeps framework code conventional while adding persistent-worker safety,
bounded diagnostics, OTLP traces, process supervision, release operations and
an MCP server for controlled automation.

## Production bootstrap

```bash
composer require pushinbr/pam-laravel
pam artisan pam:install --preset=api
pam artisan pam:check-production
pam start pam.php --workers "$(nproc)" --admin-address 127.0.0.1:3011
```

The available presets are `api`, `livewire`, `inertia` and `realtime`. The
installer publishes `config/pam.php`, `pam.processes.json`, systemd, Kubernetes,
Docker Compose and Laravel Forge assets. Run HTTP through PAM's native cluster;
queue workers, the scheduler and Nightwatch remain independent processes.

## Command map

| Task | Command |
| --- | --- |
| Readiness | `pam health` |
| Production audit | `pam check-production` |
| Memory and state leaks | `pam leaks` |
| Capacity estimate | `pam capacity --memory-mb=2048 --worker-mb=110` |
| Start managed processes | `pam up` |
| Process status | `pam status` |
| Restart one process | `pam restart queue` |
| Stop managed processes | `pam stop` |
| Local atomic release | `pam deploy /srv/app/releases/<id> --local` |
| Remote deploy | `pam deploy production` |
| All remote operations | `pam remote <action> production` |
| Nightwatch verification | `pam nightwatch` |
| Add Nightwatch process | `pam nightwatch --install-process` |
| Package registry | `pam compatibility spatie/laravel-permission --refresh` |
| Local queue autoscaling | `pam autoscale queue --cpu=80 --p95=400` |
| Forge script | `pam forge-script --output=deploy/forge-deploy.sh` |
| MCP server | `pam mcp` |

Remote actions are `deploy`, `rollback`, `status`, `logs`, `top`, `workers`,
`queues`, `scheduler` and `scale`. Scale requires `--process` and
`--instances=1..128`; logs accepts `--lines`. `rollback`, `logs`, `workers`,
`queues`, `scheduler` and `scale` also have direct `pam <action>` aliases.

## OpenTelemetry over OTLP

PAM emits W3C-trace-context-compatible spans for incoming HTTP requests,
database queries, queued jobs, Laravel commands, cache operations, outgoing
HTTP calls and reported exceptions. It exports native OTLP/HTTP JSON without
forcing a vendor SDK into the application:

```dotenv
PAM_OTLP_ENABLED=true
OTEL_SERVICE_NAME=billing-api
OTEL_SERVICE_VERSION=2026.07.25
OTEL_EXPORTER_OTLP_ENDPOINT=https://otel-collector.internal:4318
OTEL_EXPORTER_OTLP_HEADERS=Authorization=Bearer%20secret,X-Tenant=production
OTEL_EXPORTER_OTLP_COMPRESSION=gzip
```

The signal-specific standard variables
`OTEL_EXPORTER_OTLP_TRACES_ENDPOINT`, `OTEL_EXPORTER_OTLP_TRACES_HEADERS`,
`OTEL_EXPORTER_OTLP_TRACES_TIMEOUT` and
`OTEL_EXPORTER_OTLP_TRACES_COMPRESSION` take precedence. The exporter appends
`/v1/traces` when necessary, keeps a bounded in-process buffer and fails open by
default. Set `PAM_OTLP_FAIL_HARD=true` only in validation environments.

Raw cache keys are never exported. SQL summaries replace string and numeric
literals, and outgoing HTTP spans retain only the destination host. Put an
OpenTelemetry Collector beside the application and let it handle retries,
sampling and vendor credentials.

The executable smoke contract can be run independently:

```bash
php compat/laravel-smoke/telemetry-smoke.php
```

## Nightwatch

Install Nightwatch using its Laravel-compatible Composer version and configure
its token in the deployment secret store. PAM does not run the agent inside an
HTTP worker:

```bash
pam composer require laravel/nightwatch
export NIGHTWATCH_TOKEN=...
pam nightwatch --install-process
pam up
pam nightwatch
```

The diagnostic verifies package discovery, the `nightwatch:agent` command,
secret presence and a dedicated managed process.

## PAM Cloud and Forge

PAM Cloud targets expose the complete operational API:

```dotenv
PAM_CLOUD_URL=https://cloud.example.com
PAM_CLOUD_PROJECT=payments
PAM_CLOUD_TOKEN=...
PAM_REMOTE_PRODUCTION_PROVIDER=1
```

Provider values are stable integer enums: `1` is PAM Cloud and `2` is Forge.
PAM Cloud requests use
`/v1/projects/{project}/environments/{target}/{action}` with bearer
authentication. Read operations use `GET`; deploy, rollback and scale use
`POST`.

Forge deployment webhooks support deploy:

```dotenv
PAM_REMOTE_PRODUCTION_PROVIDER=2
PAM_FORGE_PRODUCTION_WEBHOOK=https://forge.laravel.com/servers/.../deploy/http
```

Generate the zero-downtime Forge script with `pam forge-script`. It uses Forge's
release creation/activation macros, installs authoritative production
dependencies, runs PAM's production audit, migrations and optimization, then
restarts queues and the PAM HTTP cluster. Remote endpoints require HTTPS unless
`PAM_REMOTE_ALLOW_HTTP=true` is deliberately enabled for a local test.

## Autoscaling

PAM has two scaling boundaries:

- `pam start pam.php --workers N` owns isolated HTTP workers behind one listener;
- `pam.processes.json` owns independent queue, scheduler, Nightwatch and other
  process instances.

`pam autoscale` reconciles a managed process between
`PAM_AUTOSCALE_MIN` and `PAM_AUTOSCALE_MAX`. It scales up by 25% when CPU exceeds
`PAM_AUTOSCALE_TARGET_CPU` or p95 latency exceeds
`PAM_AUTOSCALE_TARGET_P95_MS`; it scales down one instance only when both are
below half their targets. PAM Cloud should drive HTTP autoscaling through
`pam remote scale`.

For continuous reconciliation, point PAM at a trusted JSON endpoint:

```dotenv
PAM_AUTOSCALE_METRICS_URL=https://metrics.internal/pam/capacity
PAM_AUTOSCALE_METRICS_TOKEN=...
PAM_AUTOSCALE_COOLDOWN=60
```

The response must contain numeric `cpuPercent` and `p95Milliseconds`. `--watch`
is rejected without a live metrics endpoint; a cooldown prevents rapid
scale-up/scale-down oscillation.

## MCP for AI and automation

```json
{
  "mcpServers": {
    "pam-laravel": {
      "command": "pam",
      "args": ["mcp"]
    }
  }
}
```

The stdio server implements JSON-RPC and MCP protocol `2025-06-18`. Read-only
tools expose health, bounded metrics, production checks and managed process
state. Deploy, rollback and scale are advertised but disabled by default.

Mutations require both:

```dotenv
PAM_MCP_ALLOW_MUTATIONS=true
PAM_MCP_CONFIRMATION_TOKEN=a-long-random-value
```

Every mutating call must include that exact confirmation value. Keep mutations
off for editor and CI agents that only need diagnostics.

## Package certification

The public registry in `compatibility/laravel-packages.json` currently tracks 44
widely used packages across first-party, UI, domain, media, observability,
authentication, API and quality categories. Its integer status enum is:

1. certified;
2. provisional;
3. incompatible.

The weekly and manually dispatchable workflow creates a fresh Laravel
application for every matrix entry, resolves the current compatible package,
boots package discovery and Artisan, validates the route graph, starts two
persistent PAM workers and sends 100 requests. Each job emits an auditable JSON
artifact with the exact resolved version and timestamp. A provisional entry is
not silently promoted; only a successful certification artifact justifies
status `1`.

Run one contract locally:

```bash
scripts/certify-laravel-package.sh \
  spatie/laravel-permission '*' 13
```

## Reproducible benchmark laboratory

```bash
scripts/benchmark-laravel.sh
```

The laboratory executes the same locked Laravel route against PAM, PHP-FPM plus
Nginx, Octane plus Swoole, FrankenPHP and RoadRunner, one runtime at a time.
Every competitor receives a combined budget of two CPUs and 1 GiB; FPM and
Nginx split that same budget. Images, PHP, Composer and runtime releases are
pinned. The output includes raw rounds, p50–p99 latency, errors, container
memory, host metadata and the Git commit.

Use the scheduled GitHub workflow for public, downloadable artifacts. Do not
turn a short smoke run into a marketing claim; publish the complete raw bundle,
hardware and protocol.

## Reference applications

Two executable references live in the repository:

- `examples/laravel-saas-api` is the readable community starter. It has a real
  migration, integer-backed domain enum, model, repository, service, Form
  Request, API Resource, thin controller and feature test.
- `compat/laravel-smoke` is the broad ecosystem application used by the runtime
  suite. Its routes actively exercise Blade, Livewire, Inertia, Sanctum, Scout,
  Reverb discovery, uploads, queues, scheduler, cache, databases, Telescope and
  Pulse under persistent workers.

The first is meant to copy and extend. The second is deliberately dense and
exists to catch lifecycle, package and transport regressions.

## Release checklist

```bash
composer validate --working-dir=packages/laravel --strict
composer audit --working-dir=compat/laravel-smoke
compat/laravel-smoke/vendor/bin/phpstan analyse packages/laravel/src \
  --level=9 --no-progress
php compat/laravel-smoke/telemetry-smoke.php
cargo fmt --all -- --check
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo test --locked --all-features
```

Then run the Laravel 12/13 compatibility workflow, package certification and
benchmark smoke before creating a signed release. macOS packaging is
intentionally outside this release track.
