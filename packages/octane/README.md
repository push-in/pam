<div align="center">

# PAM Octane

### Laravel Octane, powered by Rust and Tokio.

Boot Laravel once. Keep Octane's trusted lifecycle. Let PAM own the runtime.

</div>

PAM Octane connects PAM's native HTTP transport directly to Laravel Octane's
worker. Octane continues to own application isolation, lifecycle events and
request cleanup; PAM owns PHP Embed, networking, streaming, supervision and
Tokio-powered infrastructure.

```text
HTTP → PAM / Tokio → PamClient → Octane Worker → Laravel
HTTP ← PAM / Tokio ← PamClient ← Octane Worker ← Laravel
```

## Install

PAM Octane follows PAM's versioned release train. Install a tagged stable
version for production; pre-release tags are intended for staging and feedback.

Inside an existing Laravel application:

```bash
pam composer require laravel/octane pushinbr/pam-octane
```

Start it:

```bash
pam octane:start
```

Open <http://127.0.0.1:8000>. That is the complete development setup.

Use another address or port when needed:

```bash
pam octane:start --host=0.0.0.0 --port=8080
```

For public, anonymous read endpoints, PAM can serve an explicitly opted-in
response from Rust without re-entering PHP. The cache is disabled until paths
are listed, and authenticated or cookie-bearing requests always bypass it:

```dotenv
PAM_RESPONSE_CACHE_PATHS=/api/catalog,/api/health-summary
PAM_RESPONSE_CACHE_VARY_HEADERS=accept-language
PAM_RESPONSE_CACHE_TTL_MS=30000
PAM_RESPONSE_CACHE_STALE_WHILE_REVALIDATE_MS=5000
PAM_RESPONSE_CACHE_MAX_ENTRIES=1024
PAM_RESPONSE_CACHE_MAX_BYTES=67108864
PAM_PHP_QUEUE_CAPACITY=1024
PAM_ROUTE_METRICS=false
PAM_ROUTE_METRICS_MAX_ENTRIES=256
PAM_RESPONSE_CACHE_PURGE_PATH=/__pam/cache/purge
PAM_RESPONSE_CACHE_PURGE_SECRET=a-random-secret-with-at-least-32-bytes
PAM_RESPONSE_CACHE_TAG_HEADER=x-pam-cache-tags
```

Only `GET 200` responses without `Set-Cookie`, `private`, or `no-store` are
stored. Concurrent cold requests for the same key collapse into one Laravel
execution. Query strings are part of the key.

## Why this bridge exists

PAM already embeds PHP and provides a persistent native HTTP runtime. This
package deliberately does not duplicate Octane's Laravel lifecycle. It adapts
PAM's request and response objects to Octane's public `Client` contract and then
uses the real `Laravel\Octane\Worker`.

That gives an application:

- one warm Laravel application per worker;
- Octane's request sandbox and lifecycle events;
- PAM's bounded streaming and client backpressure;
- correct cookies, uploads, `HEAD`, streamed and binary responses;
- one safe Laravel execution slot per worker;
- normal Laravel package discovery and configuration.

## Configure

The defaults are production-safe and require no configuration. To customize
limits, publish the small configuration file:

```bash
pam artisan vendor:publish --tag=pam-octane-config
```

Environment variables include:

```dotenv
PAM_HOST=127.0.0.1
PAM_PORT=8000
PAM_REQUEST_TIMEOUT_MS=30000
PAM_MAX_BODY_BYTES=2097152
PAM_MAX_RESPONSE_BYTES=268435456
```

`maxConcurrentRequests` is intentionally fixed to `1` for Laravel. Framework
managers and facades contain process-global mutable state. Scale safely with
multiple supervised PAM workers instead of overriding that boundary.

## Production

The package owns the Octane bridge; the PAM binary owns process supervision.
For a supervised production cluster, run Artisan as the worker entrypoint:

```bash
pam start artisan \
  --workers 8 \
  --max-requests 100000 \
  --admin-address 127.0.0.1:3010 \
  -- pam:octane --host=0.0.0.0 --port=8000
```

PAM then provides crash recovery, health checks, worker recycling and
generational `SIGHUP` reloads. Queue workers, Horizon and the scheduler remain
separate supervised processes.

For systemd, copy the hardened
[`pam-octane.service`](https://github.com/push-in/pam/blob/main/packaging/pam-octane.service)
unit and place deployment-specific values in
`/etc/pam/pam-octane.env`. The unit binds application HTTP to loopback and keeps
the control plane on `127.0.0.1:3010` by default.

Workloads with different latency or memory profiles can run in isolated pools:

```bash
pam octane:start \
  --ingress-address=0.0.0.0:8000 \
  --pool=api=8@/api,/graphql \
  --pool=web=4@*
```

The Rust ingress streams HTTP and WebSocket traffic, uses segment-aware longest
prefix routing, and keeps each pool's Laravel container, PHP heap, OPcache and
restart boundary independent. Exactly one `*` fallback is required. Terminate
TLS/HTTP3 at the edge when pools are enabled; internal traffic is loopback-only.

## Compatibility

The maintained matrix is:

| PHP | Laravel | Octane |
| --- | --- | --- |
| 8.4 | 12.x | 2.19+ |
| 8.4 | 13.x | 2.19+ |

PAM Octane requires the PAM runtime. Running `php artisan pam:octane` reports a
clear error because ordinary PHP CLI does not contain PAM's native server.

## Troubleshooting

Run these commands from the Laravel project root before opening a report:

```bash
pam --version
pam doctor .
pam artisan about
pam octane:status
```

- If the package reports that the PAM runtime is missing, replace `php artisan`
  with `pam artisan` or use `pam octane:start`.
- If a worker grows after repeated requests, reproduce with one worker, inspect
  request-scoped singletons and run the soak contract before lowering
  `--max-requests`.
- If requests stall, identify blocking PDO, filesystem, HTTP or extension calls;
  use PAM's cooperative clients, an isolated process pool or additional workers.
- If a reverse proxy cannot connect, keep the PAM listener private but bind it to
  the interface reachable by that proxy. Never expose the admin listener publicly.

Sanitize diagnostics: remove credentials, cookies, authorization headers,
private URLs and application data.

## Quality

```bash
composer verify
```

The suite checks request marshalling, output buffers, repeated headers, cookies,
streaming, `HEAD`, package discovery and invalid context handling. Integration
tests in the PAM repository cover the native transport and Laravel runtime.

## Runtime capabilities

- generational reload, status, stop and control-plane health;
- route-aware worker pools with crash and memory isolation;
- cooperative Fiber scheduling and HTTP/Redis clients;
- isolated bounded PDO and process pools for blocking work;
- production response cache with tags, authenticated invalidation, collapse and
  stale-while-revalidate;
- bounded route metrics plus Pulse, Telescope and OpenTelemetry integration.

PAM Octane is Apache-2.0 licensed.

Usage questions belong in PAM GitHub Discussions. Reproducible defects use the
main PAM issue tracker, and vulnerabilities must use private GitHub Security
Advisories as described in `SECURITY.md`.

## Repository model

PAM Octane is developed in `packages/octane` alongside the runtime so transport
and lifecycle changes can be tested atomically. It remains an independent,
optional Composer package: applications that do not use Laravel or Octane do
not install it.
