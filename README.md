<!-- pam:product-page:start -->
<div align="center">

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="docs/assets/readme/hero-dark.svg">
  <img src="docs/assets/readme/hero-light.svg" width="880" alt="PAM, PHP Always in Memory: one persistent PHP runtime for servers, Laravel, Android and iOS, and desktop.">
</picture>

**PHP was never the ceiling. The runtime was.**

[![Release](https://img.shields.io/github/v/release/push-in/pam?style=flat-square&label=stable)](https://github.com/push-in/pam/releases)
[![CI](https://img.shields.io/github/actions/workflow/status/push-in/pam/ci.yml?branch=main&style=flat-square&label=CI)](https://github.com/push-in/pam/actions/workflows/ci.yml)
![PHP](https://img.shields.io/badge/PHP-8.5-777BB4?style=flat-square&logo=php&logoColor=white)
![Rust](https://img.shields.io/badge/Rust-Tokio-000000?style=flat-square&logo=rust&logoColor=white)
[![Docs](https://img.shields.io/badge/docs-pam--docs-2a78d6?style=flat-square)](https://push-in.github.io/pam-docs/introduction/)
![License](https://img.shields.io/github/license/push-in/pam?style=flat-square)

**[Documentation](https://push-in.github.io/pam-docs/introduction/) · [Quick start](#quick-start) · [Benchmarks](#benchmarks) · [Ecosystem](#the-ecosystem) · [Discussions](https://github.com/push-in/pam/discussions)**

</div>

---

## Why this exists

Classic PHP boots the interpreter, loads Composer, builds your framework, runs one
request and throws it all away. Then it does it again.

**PAM keeps it.** The Zend Engine and your Composer application boot once inside a
supervised Rust + Tokio process. Rust handles the network, timers, TLS and async I/O.
Each request runs in its own PHP Fiber against an application that is already warm.
The same runtime then powers native Android, iOS and desktop apps written in PHP.

It is **not** a PHP fork, a new language or a Composer replacement. Composer stays
Composer. Laravel stays Laravel. Your code stays PHP.

## What you can build

| | |
| --- | --- |
| **APIs and services** | HTTP/1.1, HTTP/2, HTTP/3 and WebSocket on one port, with TLS, limits and metrics built in |
| **Laravel without PHP-FPM** | A warm Laravel kernel with per-request sandboxes, or your existing Octane app on PAM's transport |
| **Production clusters** | A pm2-style manager with zero-downtime reload, canary traffic, health checks and rollback |
| **Android and iOS apps** | Real Android Views and UIKit controls driven by PHP components, with no JavaScript runtime and no web view |
| **Desktop apps** | Self-contained Linux, macOS and Windows apps, with a native GTK 4, AppKit and Win32 renderer coming in 2.0 |

## Quick start

Install one verified runtime. You don't need a global PHP, a global Composer, PHP-FPM or a Rust toolchain.

```console
$ curl --proto '=https' --proto-redir '=https' --tlsv1.2 \
    --connect-timeout 15 --max-time 60 --max-filesize 1048576 -fsSL \
    https://github.com/push-in/pam/releases/latest/download/install.sh | sh

$ pam init my-api --no-interaction     # API preset: pushinbr/pam-http + tests
$ cd my-api && pam dev                 # hot reload on http://127.0.0.1:3000

$ curl http://127.0.0.1:3000/api/ping
{"message":"pong"}
```

The generated `index.php` is plain PHP:

```php
$app = new App;

$app->get('/api/ping', static fn (Request $request, Response $response): Response => $response->json([
    'message' => 'pong',
]));
$app->listen((int) (getenv('PAM_PORT') ?: 3000));
```

The same CLI creates any other kind of project:

```bash
pam init my-laravel-app --template laravel  # persistent Laravel
pam init my-native-app  --template native   # Android + iOS
pam init my-desktop-app --template desktop  # Linux + macOS + Windows
pam init my-product     --template product  # server + native + desktop, one workspace
```

<!-- pam:product-page:end -->

## How it works

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="docs/assets/readme/architecture-dark.svg">
  <img src="docs/assets/readme/architecture-light.svg" width="960" alt="PAM architecture: a supervised master runs worker processes; in each worker Rust and Tokio own the network and I/O while the Zend Engine keeps the Composer application in memory and runs each request in a Fiber; Composer packages add HTTP, Laravel, Native and Desktop.">
</picture>

- **Boot once.** PHP Embed starts with the real process arguments. Composer and your
  entrypoint load a single time, and your routes and callbacks register before the listeners open.
- **Fibers, not threads.** Each request runs in an isolated Fiber. A native wait such as a socket,
  timer, HTTP call or process suspends to Tokio and resumes the same Fiber with the result.
  One Zend NTS engine belongs to one worker thread, and nothing calls into PHP from a foreign thread.
- **Clean between requests.** Request state is cleaned up deterministically, and allocator caches are released on a schedule.
- **A small core.** The runtime owns execution, processes, transports and async I/O. Routing,
  Laravel, Native, Desktop and every product feature live in Composer packages that add their own `pam` commands.

Read [Architecture](docs/architecture.md), [Async runtime](docs/async-runtime.md) and the versioned
[Native API](docs/native-api.md) for the internals.

## Runtime and HTTP

| Capability | What ships |
| --- | --- |
| Protocols | HTTP/1.1, HTTP/2 over ALPN, HTTP/3 over QUIC on the same port (with `Alt-Svc`), RFC 6455 WebSocket with compression and resumable sessions |
| TLS | Rustls, verified TLS in the built-in HTTP client, redirect credential stripping |
| Limits | Body, header, response and chunk caps; header/body/request deadlines; slowloris defense; token-bucket rate limiting; CORS; trusted proxies |
| Streaming | SSE and chunked responses with bounded queues, backpressure and cancellation |
| PHP | Composer, PSR-7/15/17 bridges, PSR-3 logging, Pest and PHPUnit through `pam test`, Amp/Revolt through Fibers |
| Packaging | `pam build` produces a relocatable bundle with the app, `vendor`, runtime, exact `libphp` and a SHA-256 manifest |

```php
$app->listen(443, '0.0.0.0', [
    'tlsCert' => '/etc/pam/fullchain.pem',
    'tlsKey' => '/etc/pam/private-key.pem',
    'http3' => true,
    'maxBodyBytes' => 2 * 1024 * 1024,
    'requestTimeoutMs' => 30_000,
    'rateLimitPerSecond' => 200,
    'websocketMaxConnections' => 10_000,
]);
```

Every option is documented in [Production operations](docs/production.md) and
[Server transports](docs/server-transports.md).

## Laravel and Octane

```console
$ pam init my-laravel-app --template laravel
$ cd my-laravel-app
$ pam dev pam.php
⚡ Laravel booted once and listening on http://127.0.0.1:3000
```

PAM downloads the official skeleton, installs normal Composer dependencies and keeps
Laravel intact in `vendor`. Each request gets an isolated application sandbox while the
framework stays warm. Artisan is still Artisan: `pam artisan migrate`, `pam artisan queue:work`.

The executable compatibility matrix covers **Laravel 12 and 13** with SQLite, MySQL, PostgreSQL,
Redis, queues, Sanctum, Scout, Livewire, Inertia, Reverb, Telescope and Pulse.

Already on Octane? Keep Octane's lifecycle and swap in PAM's Rust transport:

```bash
pam composer require laravel/octane pushinbr/pam-octane
pam octane:start
```

The bridge adds route-aware worker pools, cooperative Fiber HTTP/Redis clients, bounded PDO and
process pools, and an opt-in response cache with tags, collapse and stale-while-revalidate.
See the [Laravel guide](https://push-in.github.io/pam-docs/laravel/overview/), the
[Octane package](packages/octane/README.md) and the [Octane security contract](docs/octane-security.md).

## Process manager

Development is one process. Production is a supervised cluster:

```bash
pam start index.php --workers 10 --admin-address 127.0.0.1:3010
```

The master starts workers with `SO_REUSEPORT` and replaces crashed workers with backoff.
It watches request deadlines outside the Zend Engine and replaces a worker stuck in
blocking PHP. On `SIGTERM` it drains, and on `SIGHUP` it performs a readiness-gated
generational reload that keeps the healthy generation alive if the new one fails to boot.
`/live`, `/startup`, `/ready` and `/metrics` are served outside PHP.

For many applications, `pamd` works like pm2 for PHP:

```bash
pam up index.php --name api --workers 8 \
  --health-check-url http://127.0.0.1:8080/health
pam reload api                                  # zero-downtime generational reload
pam scale api 12
pam logs api --both --follow
pam monit                                       # live health and capacity
pam deploy api /srv/api/releases/2026-11-01     # readiness-gated release
pam rollback api                                # back to the previous healthy release

pam traffic:set edge --candidate 127.0.0.1:8082 --weight-bps 500     # 5% canary
pam traffic:evaluate edge --min-candidate-requests 1000 --max-candidate-error-bps 50
pam traffic:promote edge
```

It also covers declarative `pam.toml` with `pam plan` and `pam apply`, cgroup-v2 memory and task
limits, systemd startup, a private live dashboard and bounded history.
Read the [Process manager](docs/process-manager.md) guide.

## PAM Native: Android and iOS in PHP

<p align="center">
  <img src="docs/assets/readme/native-showcase.png" width="900" alt="Four screens of the public PAM Native showcase on Android: marketplace, finance dashboard, chat and offline field operations.">
</p>
<p align="center"><sub>The public <a href="https://github.com/push-in/pam-native/tree/main/examples/showcase">PAM Native showcase</a> running on an Android phone. Every control is a real Android View.</sub></p>

PAM Native has no DOM, no web view and no JavaScript runtime between your state and the
platform. PHP describes the UI. Rust validates it, diffs it and lays it out with Yoga. Kotlin and Swift then
commit only the changed views on the UI thread.

```php
final class Counter extends Component
{
    #[State]
    public int $count = 0;

    public function increment(): void
    {
        $this->count++;
    }
}
?>

<template>
    <Column class="counter">
        <Text class="value">{{ $count }}</Text>
        <Button label="Increment" @press="increment" />
    </Column>
</template>

<style scoped>
    .counter { padding: 24px; gap: 16px; align-items: center; }
    .value { font-size: 56px; font-weight: 800; }
</style>
```

```text
native tap → PHP action → #[State] change → Rust diff → UI-thread commit
```

PAM Native includes native stacks, tabs, drawers, sheets and gestures; recycled lists built for
10,000+ rows; hot reload that keeps navigation and focus where the platform allows; UI-thread animations; and a typed
plugin SDK for your own Kotlin and Swift modules. Start with `pam init orbit --template native`, then
read the [Native docs](https://push-in.github.io/pam-docs/native/overview/) and the
[pam-native repository](https://github.com/push-in/pam-native).

## PAM Desktop

<p align="center">
  <img src="docs/assets/readme/one-codebase.png" width="820" alt="The same marketplace screen from the PAM showcase on an Android phone and in a PAM Desktop window on Linux.">
</p>
<p align="center"><sub>The same showcase screen in PAM Native on Android (left) and in the PAM Desktop 2.0 preview on Linux with GTK 4 (right). Both are written as PHP components with the same syntax.</sub></p>

**PAM Desktop 1.2** is the released line ([v1.2.9](https://github.com/push-in/pam-desktop/releases)).
It runs typed PHP application logic behind a Rust capability boundary and renders local
HTML/CSS in native windows through Servo, with no Electron or Node.js. It ships with multiple
windows, background jobs, hot reload, crash recovery, signed updates with rollback, and DEB, DMG and MSIX packaging.
Power is explicit: filesystem roots, dialogs, clipboard and notifications are granted one by one.

**PAM Desktop 2.0** is in development on the public
[`feat/native-2.0`](https://github.com/push-in/pam-desktop/tree/feat/native-2.0) branch, and it removes the
web view entirely. PHP components with the same syntax as PAM Native drive **GTK 4 on Linux, AppKit on
macOS and Win32 + Direct2D on Windows**. Rust lays out and diffs the tree, so every control
is a platform view.

<p align="center">
  <img src="docs/assets/readme/desktop-showcase.png" width="900" alt="PAM Desktop 2.0 preview: marketplace, finance and chat screens on Linux with GTK 4, and a 2,000-row virtualized list on macOS with AppKit.">
</p>
<p align="center"><sub>PAM Desktop 2.0 preview, captured from the public <code>examples/</code>: showcase screens on Linux with GTK 4, plus a 2,000-row virtualized list on macOS with AppKit (bottom right).</sub></p>

See the [Desktop docs](https://push-in.github.io/pam-docs/desktop/overview/) and the
[pam-desktop repository](https://github.com/push-in/pam-desktop).

## The ecosystem

Product features ship as Composer packages. You install them with `pam composer require`, and each one adds its own commands.

| Core package | Responsibility |
| --- | --- |
| [`pushinbr/pam-http`](packages/http) | Express-like routing, middleware, controllers, validation and resources |
| [`pushinbr/pam-socket`](packages/socket) | WebSocket events and rooms |
| [`pushinbr/pam-http-psr`](packages/http-psr) | PSR-7, PSR-15 and PSR-17 interoperability |
| [`pushinbr/pam-octane`](packages/octane) | Laravel Octane bridge |
| [`pushinbr/pam-laravel`](https://github.com/push-in/pam-laravel) | Laravel production tooling and lifecycle safety |
| [`pushinbr/pam-native`](https://github.com/push-in/pam-native) | Android/iOS host and PHP application framework |
| [`pushinbr/pam-desktop`](https://github.com/push-in/pam-desktop) | Desktop host and PHP application framework |

**38 public PAM Native plugins** are listed in the [ecosystem catalog](config/ecosystem-packages.json):

| Area | Plugins (`pushinbr/pam-native-*`) |
| --- | --- |
| UI and graphics | [ui](https://github.com/push-in/pam-native-ui) · [canvas](https://github.com/push-in/pam-native-canvas) · [charts](https://github.com/push-in/pam-native-charts)† · [svg](https://github.com/push-in/pam-native-svg) · [3d](https://github.com/push-in/pam-native-3d) · [gpu](https://github.com/push-in/pam-native-gpu) · [lottie](https://github.com/push-in/pam-native-lottie) · [image](https://github.com/push-in/pam-native-image) · [maps](https://github.com/push-in/pam-native-maps) |
| Camera and media | [camera](https://github.com/push-in/pam-native-camera) · [scanner](https://github.com/push-in/pam-native-scanner) · [media](https://github.com/push-in/pam-native-media) · [video](https://github.com/push-in/pam-native-video) · [audio](https://github.com/push-in/pam-native-audio) · [background-transfer](https://github.com/push-in/pam-native-background-transfer) · [share-extension](https://github.com/push-in/pam-native-share-extension) |
| Device and OS | [bluetooth](https://github.com/push-in/pam-native-bluetooth) · [nfc](https://github.com/push-in/pam-native-nfc) · [health](https://github.com/push-in/pam-native-health) · [intents](https://github.com/push-in/pam-native-intents) · [widgets](https://github.com/push-in/pam-native-widgets) · [live-activities](https://github.com/push-in/pam-native-live-activities) · [calls](https://github.com/push-in/pam-native-calls)† |
| Data and realtime | [nitro](https://github.com/push-in/pam-native-nitro) · [sync](https://github.com/push-in/pam-native-sync) · [sync-laravel](https://github.com/push-in/pam-native-sync-laravel) · [realtime](https://github.com/push-in/pam-native-realtime) · [webrtc](https://github.com/push-in/pam-native-webrtc)† |
| Identity and commerce | [auth](https://github.com/push-in/pam-native-auth) · [payments](https://github.com/push-in/pam-native-payments) · [subscriptions](https://github.com/push-in/pam-native-subscriptions) |
| Operations and quality | [observability](https://github.com/push-in/pam-native-observability) · [analytics](https://github.com/push-in/pam-native-analytics) · [firebase](https://github.com/push-in/pam-native-firebase) · [feature-flags](https://github.com/push-in/pam-native-feature-flags) · [devtools](https://github.com/push-in/pam-native-devtools) · [testing](https://github.com/push-in/pam-native-testing) · [plugin-kit](https://github.com/push-in/pam-native-plugin-kit) |

<sub>† No tagged release yet. The full contract is in [docs/ecosystem.md](docs/ecosystem.md) and [docs/packages.md](docs/packages.md).</sub>

## Benchmarks

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="docs/assets/readme/benchmark-laravel-dark.svg">
  <img src="docs/assets/readme/benchmark-laravel-light.svg" width="920" alt="Laravel throughput and p99 latency, one worker each. PAM 2.2.9: 1,666 / 1,267 / 1,283 / 1,551 requests per second for minimal JSON, Blade, SQLite and large JSON; FrankenPHP: 1,573 / 1,172 / 1,184 / 1,425; OpenSwoole: 1,222 / 940 / 971 / 1,139.">
</picture>

The chart shows the most recent hosted run of the [Laravel comparison protocol](benchmarks/octane/README.md).
It is [Performance evidence run 41623674573](https://github.com/push-in/pam/actions/runs/41623674573)
from 2026-11-03, on the PAM **2.2.9** release commit `91d3ba8`. All three servers ran the same Laravel
fixture with **one worker**. Each request executes the full Laravel kernel.

| Scenario (req/s, median) | PAM 2.2.9 | FrankenPHP | OpenSwoole 25.2 |
| --- | ---: | ---: | ---: |
| Minimal JSON | **1,666** | 1,573 | 1,222 |
| Blade view | **1,267** | 1,172 | 940 |
| SQLite query | **1,283** | 1,184 | 971 |
| Large JSON | **1,551** | 1,425 | 1,139 |
| Process-tree RSS, p50 | 151.5 MB | 146.6 MB | 321.6 MB |

**Method.** The run used a GitHub-hosted runner (AMD EPYC 7763, 4 vCPUs). The server was pinned to 2 CPUs and `wrk` to the other 2.
`wrk` ran with 4 threads and 128 connections, for 5 rounds of 15 s each after a 5 s warmup, with the scenario order rotated every round.
The FrankenPHP image is pinned by digest. OPcache and the tracing JIT were on everywhere, and every runtime finished with zero errors.

**Read it honestly:**

- The comparison is not like-for-like. PAM ran PHP **8.5.12**, while the FrankenPHP and OpenSwoole containers ran PHP **8.4.24**.
- The protocol's own release gate did **not** pass. Runtime preflight failed on the PHP mismatch, and the tree was flagged as dirty.
  This is published evidence, not a certified result.
- p99 is between 81 and 146 ms because 128 connections queue on a single worker. Compare the runtimes against each other, not
  against your production latency. PAM's Blade and SQLite p99 values went over the protocol's 100 ms gate.
- The opt-in PAM response cache served a cacheable route at **71,804 req/s** (p99 2.7 ms). That run measures different work
  from full Laravel execution, so it is not in the chart.

Run it yourself with `benchmarks/octane/run.sh`, or dispatch the
[Performance evidence workflow](.github/workflows/evidence.yml). The
[benchmark protocol](benchmarks/README.md) also covers process-manager crash recovery, including a
clean hosted comparison against PM2 that is documented in [docs/process-manager.md](docs/process-manager.md).

## Observability

- Prometheus metrics for requests, errors, latency, bytes, WebSockets, backpressure, event-loop lag, RSS, PHP memory and Fibers.
- W3C `traceparent`, request IDs and `Server-Timing`. Sampled JSON access logs keep every 5xx.
- **OTLP/HTTP traces** certified against the official OpenTelemetry Collector, across Server, Native and Desktop. See
  [Observability](docs/observability.md).
- Live tools: `pam top`, `pam diagnostics`, `pam heap`, `pam fibers`, `pam connections`, `pam profile` and `pam trace`.

## Security

- **Verified installs and updates.** `install.sh` checks SHA-256 for every artifact. `pam self-update` installs
  only releases that a signed update manifest authorizes against a pinned signing identity. Release bundles carry
  build attestations.
- **Bounded by default.** Every queue, body, stream, payload and process output has a limit and a deadline.
- **The admin plane is private by default.** It is off unless you set `--admin-address`. A non-loopback address requires
  a bearer token, and the token is removed from PHP worker environments.
- **Least privilege on Desktop.** Commands must be registered, filesystem access is limited to named roots, and the
  local bridge uses a per-process token with origin checks.
- **Audited.** CI runs `cargo audit`, `composer audit`, fuzzing, memory soak and Valgrind workflows.

Report vulnerabilities privately as described in [SECURITY.md](SECURITY.md).

## Known limitations

- **One PHP thread per worker.** Requests interleave at suspension points, but only one runs PHP bytecode at a time
  inside a worker. Scale with `--workers N`.
- **Laravel is serialized per worker.** Framework managers and facades are process-global, so PAM enforces
  `maxConcurrentRequests=1` for the Laravel host. Add workers to add concurrency.
- **Blocking extensions still block.** Use native PAM I/O, a cooperative library, `ProcessPool` or more workers.
- **Amp/Revolt are bridged, not transformed.** Their drivers run through Fibers and can occupy a worker.
- **WebSocket clients must reconnect after a reload.** They can resume with `sessionId` and `resumeToken`. Socket.IO
  (Engine.IO) is not implemented, and HTTP/3 covers request/response only.
- **Persistent-app rules apply.** Don't keep request state in globals or singletons.
- **Server hosts are Linux and macOS.** On Windows, use WSL for server projects.

Start with staging, soak tests, representative traffic and a tested rollback plan. Read the
[project status](https://push-in.github.io/pam-docs/project/status/) before production use.

## Documentation and community

| | |
| --- | --- |
| **Learn** | [Documentation](https://push-in.github.io/pam-docs/introduction/) · [Getting started](docs/getting-started.md) · [CLI reference](docs/cli-reference.md) |
| **Operate** | [Production](docs/production.md) · [Process manager](docs/process-manager.md) · [Observability](docs/observability.md) |
| **Build** | [Laravel](docs/laravel.md) · [Packages](docs/packages.md) · [Extending the CLI](docs/extending-cli.md) · [Android](docs/android.md) · [iOS](docs/ios.md) |
| **Talk** | [Discussions](https://github.com/push-in/pam/discussions) · [Issues](https://github.com/push-in/pam/issues) · [Support](SUPPORT.md) |
| **Contribute** | [Contributing](CONTRIBUTING.md) · [Governance](GOVERNANCE.md) · [Roadmap](ROADMAP.md) · [Changelog](CHANGELOG.md) |

## License

PAM is free and open-source software under the [Apache License 2.0](LICENSE). You may use, modify
and distribute it, including commercially. See the plain-language [licensing guide](LICENSING.md).

---

<div align="center">

**Keep the language. Keep the ecosystem. Upgrade the runtime.**

</div>
