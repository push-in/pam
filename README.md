<div align="center">

# ⚡ Pam

### PHP, Always in Memory.

**A persistent, event-driven PHP application runtime powered by Rust, Tokio, Fibers, and the Zend Engine.**

Write elegant PHP. Keep Composer. Serve HTTP, WebSockets, and asynchronous I/O from memory—without rebuilding your application for every request.

![Status](https://img.shields.io/badge/status-experimental-f59e0b?style=flat-square)
![PHP](https://img.shields.io/badge/PHP-8.4-777BB4?style=flat-square&logo=php&logoColor=white)
![Rust](https://img.shields.io/badge/Rust-1.88%2B-000000?style=flat-square&logo=rust&logoColor=white)
![HTTP](https://img.shields.io/badge/HTTP-1.1%20%7C%202%20%7C%203-2563eb?style=flat-square)
![License](https://img.shields.io/badge/license-MIT-22c55e?style=flat-square)

</div>

---

Pam brings the long-lived, event-driven runtime model to PHP while preserving the ecosystem PHP developers already trust.

It embeds PHP through the official Embed SAPI, loads your application and Composer autoloader once, and keeps them alive inside a Rust runtime. Incoming requests run in isolated PHP Fibers; native I/O is scheduled by Tokio; HTTP and WebSocket connections stay in memory; production workloads scale through supervised workers.

Pam is **not a framework**, **not a Composer replacement**, and **not a new language**. The binary is the runtime layer beneath your application; optional first-party features are ordinary Composer packages.

> [!IMPORTANT]
> Pam is currently experimental (`0.1.4`). Its integration suite exercises the contracts documented here, but read [Known limitations](#known-limitations) before evaluating it for production.

**Explore:** [Quick start](#quick-start) · [Composer](#composer-stays-composer) · [Async I/O](#async-php-backed-by-tokio) · [WebSockets](#websockets-on-the-same-port) · [Production](#built-for-production-operations) · [Performance](#performance) · [Architecture](#how-it-works) · [Limitations](#known-limitations)

## Why Pam?

Traditional PHP deployment is excellent at isolation, but each request usually rebuilds a meaningful part of the application: bootstrap files, dependency injection containers, configuration, routes, middleware, and framework state.

Pam changes the lifecycle:

```text
Traditional PHP                         Pam

request                                 process starts
  ├─ bootstrap PHP                        ├─ start Zend + Tokio
  ├─ load Composer                        ├─ load Composer
  ├─ build application                    ├─ build application once
  ├─ execute handler                    request 1 ─┐
  └─ discard everything                 request 2 ─┼─ isolated Fibers
request                                   request N ─┘
  └─ repeat all of the above              process stays alive
```

The result is a runtime designed for APIs, real-time systems, streaming, background I/O, and high-throughput services—with PHP syntax and Composer packages intact.

## What you get

| Area | Where it lives |
| --- | --- |
| Runtime core | Persistent Zend Engine, Tokio scheduler, Fibers, HTTP transport, streams, async I/O, process and diagnostics |
| `pushinbr/pam-api` | Expressive routing, route parameters, middleware, error handling and package discovery |
| `pushinbr/pam-socket` | RFC 6455 events, rooms, broadcasts, acknowledgements, adapters and resume support |
| `pushinbr/pam-psr-bridge` | PSR-7, PSR-15 and PSR-17 interoperability using the official interfaces |
| `pushinbr/pam-testing` | Fast in-memory HTTP client and fluent response assertions |
| `pushinbr/pam-core-api` | Small, versioned contracts for packages that extend Pam |
| Pam Desktop | Separate Servo host plus a typed Composer package for building desktop applications with PHP |
| Composer | Normal `composer.json`, `composer.lock`, PSR-4, custom vendor directories, and `vendor/autoload.php` |
| Production | Master/worker mode, crash recovery, watchdog, graceful drain, worker recycling, generational reload |
| Operations | Prometheus metrics, health endpoints, structured logs, tracing, profiling, diagnostics, live `pam top` |
| Security | Request limits, timeouts, slowloris protection, verified TLS, CORS, rate limiting, trusted proxies |
| Distribution | Relocatable production bundles with application, `vendor`, runtime, exact `libphp`, and SHA-256 manifest |
| Developer experience | One binary, expressive PHP API, project generator, hot reload, test runner, doctor, benchmark tooling |

## Quick start

### 1. Install the runtime

Pam currently ships prebuilt Linux releases for x86_64 and ARM64. Official
archives target glibc 2.35 or newer and bundle non-system shared libraries used
by the runtime and its reviewed PHP extensions.

```bash
curl --proto '=https' --tlsv1.2 --fail --silent --show-error --location \
  --output pam-install.sh \
  https://github.com/push-in/pam/releases/latest/download/install.sh
sh pam-install.sh
rm pam-install.sh
pam --version
pam doctor .
```

The release contains the PAM binary, its exact private PHP Embed library, a
reviewed set of common PHP extensions, and an isolated INI tree. End users do
**not** install PHP, Composer, Rust, `php-config`, development headers or an FPM
service. The installer verifies the release SHA-256, rejects unsafe archive paths
and installs without root under `~/.local`.

Composer is downloaded and signature-verified inside PAM's Embed SAPI only when a
project first needs it.

PAM never reads the host PHP configuration for an official release. Extra
extension configuration can be added explicitly with `PAM_PHP_INI_SCAN_DIR`;
there is no accidental dependency on `/etc/php`.

<details>
<summary>Building PAM itself from source</summary>

Runtime contributors need Rust 1.88+, a C toolchain, matching PHP 8.4 development
headers and the Embed library:

```bash
sudo apt-get install -y build-essential php8.4-dev libphp8.4-embed
cargo build --locked --release
```

Use `PHP_CONFIG` and `PAM_PHP_LIB_DIR` for a custom build toolchain. These are
build-time requirements and are not required on machines using an official PAM
release.

</details>

### 2. Create an application

```bash
pam init my-api --template api
cd my-api
pam dev index.php
```

The generated application exposes `GET /api/ping` on port `3000`.

```bash
curl http://127.0.0.1:3000/api/ping
```

```json
{"message":"pong"}
```

That is a persistent PHP server with hot reload. No FPM pool, no framework bootstrap per request, and no second package ecosystem.

`pam init` installs dependencies automatically through Composer running inside Pam's
Embed SAPI. The project and lockfile remain standard Composer artifacts; no external
PHP CLI or global Composer installation is required.

> [!NOTE]
When Pam is built from this monorepo, generated API projects use a Composer path
repository for the local first-party packages. Published binaries use their normal
Packagist versions.

### Laravel in one command

```bash
pam init my-laravel-app --template laravel
cd my-laravel-app
pam dev pam.php
```

Pam downloads the official Laravel skeleton, installs its normal Composer
dependencies, runs package discovery, generates the application key and configures
a stateless `GET /api/ping` route. The `Pam\Laravel` host is compiled into the
binary: Laravel itself remains untouched in `vendor`, boots once, and receives an
isolated application sandbox for every request.

The executable matrix validates Laravel 12 and 13 with SQLite, MySQL,
PostgreSQL, Redis, database queues, Artisan, Sanctum, Scout, Livewire, Inertia,
Reverb, Telescope and Pulse. Horizon and Socialite are tested on Laravel 12,
the version accepted by their current stable Composer constraints. The supported
contract is maintained Laravel 12 and current Laravel 13.

Artisan runs through Pam with a real CLI SAPI identity, normal arguments, exit
codes, standard streams, and Laravel's console environment:

```bash
pam artisan migrate
pam artisan route:list
pam artisan test
pam artisan queue:work
```

Add the native Socket transport to either preset:

```bash
pam init realtime-api --template api --socket
pam init realtime-laravel --template laravel --socket
```

Run `pam init` without a template in a terminal for the interactive preset picker.
Use `--no-install` to generate/download source without resolving dependencies.
See [Laravel on Pam](docs/laravel.md) for lifecycle and production details.

### Mobile presets

The CLI offers two native mobile starting points:

```bash
pam init native-core --template mobile
pam init native-ui --template mobile-ui
```

`mobile` starts with the explicit PAM Native PHP tree and no component-library
dependency. `mobile-ui` installs `pushinbr/pam-mobile-ui`, enables its system
theme and generates a `Hello.pam.php` screen using the official accessible
components. Both presets render through the same PAM Native element tree, Rust
diff engine and Android UI-thread commit path.

### Desktop applications with Servo

Pam Desktop lives in its own repository because its release cadence, native
toolchain and threat model differ from the server runtime. The Pam CLI still
knows how to create a polished starter:

```bash
pam init my-desktop-app --template desktop
cd my-desktop-app
pam desktop doctor .
pam desktop dev .
pam desktop build .
```

`pam desktop` is the public command; it delegates to the separately distributed
`pam-desktop` host and identifies the current Pam executable to its PHP worker.
The 1.0 starter demonstrates the stable Linux API, multiple windows,
bidirectional events, command timeouts, crash recovery, development hot reload
and explicit native
capabilities. Window configuration and application policy remain in PHP, local
HTML/CSS/JavaScript renders directly with Servo, and the Rust host supervises a
versioned JSON-lines protocol. Named filesystem roots, dialogs, clipboard,
notifications and drag-and-drop grants are opt-in. Application identity,
category, icon and signed-update policy also live in a typed PHP manifest.
Menus, tray, close-to-tray behavior, global shortcuts, background jobs and
composable PHP plugins are configured through the same public application API.
Native Rust plugins remain process-isolated and can be scaffolded with
`pam desktop plugin new`:

```php
Manifest::create('com.pushin.pam-hello', 'Pam Hello', '1.0.0')
    ->publisher('Pushin')
    ->category(ApplicationCategory::Development);
```

`pam desktop build` creates a self-contained, update-ready Linux package.
`--format deb` adds a Debian package. Bundles include the Pam worker, PHP
runtime libraries, Servo host, vendored PHP application and native plugins,
Linux metadata, icon and a SHA-256 integrity manifest. Feed signing and
automatic updates remain behind explicit PHP policy and a pinned Ed25519 public
key. Windows/macOS packager code is preserved upstream, but the current release
pipeline generates Linux x86-64 artifacts only.

Native capabilities remain explicit:

```php
$app->capabilities(
    Capabilities::none()
        ->filesystem(FileSystemRoot::readWrite('data', __DIR__.'/storage'))
        ->dialogs()
        ->clipboard()
        ->notifications()
        ->dragAndDrop(),
);
```
Browser code can call only commands explicitly registered by the application:

```php
$app->command('greet', static fn (CommandContext $command): CommandResult =>
    CommandResult::success([
        'message' => 'Olá, '.$command->string('name', 'mundo').'.',
    ])
);
```

The local bridge binds to a random loopback port, requires a cryptographically
random per-process token and matching origin, applies a restrictive CSP, and
prevents static assets from escaping the project. Deadlined or cancelled
commands terminate the compromised worker and prepare a fresh generation
without replaying possible side effects. Pam Desktop 1.x freezes public API `1`,
worker protocol `6` and the Rust plugin SDK `1` for Linux x86-64. Servo 0.4
continues evolving; stability of PAM's contracts is not a claim of
feature-for-feature Electron parity.

## A small core, a Composer ecosystem

The split is intentional. Pam's binary plays the role of Node's runtime and standard library. It owns process lifecycle, the event loop, network transports, native I/O, memory boundaries, diagnostics and the PHP Embed ABI. It does **not** own application routing or package installation.

```text
pam binary
├── PHP Embed + Fibers + Tokio
├── HTTP/TLS/QUIC + low-level Request/Response server
├── async I/O, streams, files, DNS, processes and signals
└── lifecycle, workers, health, metrics and diagnostics

Composer
├── pushinbr/pam-api          routing + middleware (the Express-like layer)
├── pushinbr/pam-socket       realtime events (the Socket.IO-like layer)
├── pushinbr/pam-psr-bridge   standards interoperability
├── pushinbr/pam-testing      in-memory application tests
├── pushinbr/pam-desktop  desktop application model (separate repository)
└── every existing compatible PHP package
```

The core can serve HTTP without a framework:

```php
use Pam\Http\Request;
use Pam\Http\Response;
use Pam\Http\Server;

Server::create(static fn (Request $request, Response $response): Response =>
    $response->json(['path' => $request->path])
)->listen(3000);
```

Install only the higher-level pieces your application needs:

```bash
pam composer require pushinbr/pam-api
pam composer require pushinbr/pam-socket          # optional
pam composer require pushinbr/pam-psr-bridge      # optional
pam composer require --dev pushinbr/pam-testing   # optional
```

See [Packages and extension model](docs/packages.md) for stability, discovery and publishing rules.

## The API programming model

`pushinbr/pam-api` is the optional, Express-like API:

```php
<?php

declare(strict_types=1);

use Pam\App;
use Pam\Http\Request;
use Pam\Http\Response;

$app = new App();

$app->get('/api/users', static function (Request $request, Response $response): Response {
    return $response->json([
        'search' => $request->getQuery('search'),
        'requestId' => $_SERVER['PAM_REQUEST_ID'],
    ]);
});

$app->post('/api/messages', static function (Request $request, Response $response): Response {
    return $response->json([
        'accepted' => true,
        'message' => $request->json(),
    ], 201);
});

$app->listen(3000);
```

Run it:

```bash
pam dev index.php
curl 'http://127.0.0.1:3000/api/users?search=ada'
```

Pam also supports the PHP behavior existing applications expect:

- `$_GET`, `$_POST`, `$_COOKIE`, `$_FILES`, `$_REQUEST`, and `$_SERVER`;
- `php://input`;
- JSON, URL-encoded forms, multipart bodies, and uploads;
- repeated response headers;
- `header()`, `setcookie()`, and `http_response_code()`;
- native PHP sessions.

Superglobals, output buffers, headers, sessions, uploads, Fiber context, and request-scoped resources are restored or cleaned after every request.

## Composer stays Composer

Pam does not ship a package registry or a competing dependency format.

```bash
pam composer require guzzlehttp/guzzle
pam composer install
pam doctor .
pam test .
```

Pam discovers the nearest `composer.json`, respects `config.vendor-dir`, and loads the normal Composer autoloader. Your lockfile remains the source of truth.

Pam packages use the same mechanism. A third-party package can publish a service provider under `extra.pam.providers`; `pushinbr/pam-api` discovers it from Composer's installed metadata and writes an atomic cache under `.pam/cache`. Set `PAM_DISABLE_PACKAGE_DISCOVERY=1` for fully explicit registration.

The executable compatibility project currently covers real behavior from:

- Amp 3 and Revolt;
- ReactPHP;
- Guzzle 8;
- Monolog;
- OpenTelemetry;
- Illuminate Container, Events, and Pipeline 13;
- Symfony HttpFoundation and HttpKernel 8;
- Slim 4;
- PHPUnit and Pest;
- PSR-3, PSR-7, PSR-15, and PSR-17.

Pure PHP packages generally work through normal autoloading. Extensions work when they are loaded by the Embed SAPI and compatible with a persistent process. Packages that assume FPM, Apache, CLI-only behavior, or request-local global state must be validated.

Read the full [Composer compatibility contract](docs/compatibility.md).

## PSR middleware and applications

Install the official PSR interfaces through Composer and use existing middleware contracts directly:

```php
<?php

declare(strict_types=1);

use Pam\App;
use Pam\Http\Psr7\Factory;
use Psr\Http\Message\ResponseInterface;
use Psr\Http\Message\ServerRequestInterface;
use Psr\Http\Server\RequestHandlerInterface;

$factory = new Factory();
$app = new App();

$app->handler(new class($factory) implements RequestHandlerInterface {
    public function __construct(private readonly Factory $factory)
    {
    }

    public function handle(ServerRequestInterface $request): ResponseInterface
    {
        return $this->factory
            ->createResponse(200)
            ->withHeader('content-type', 'application/json')
            ->withBody($this->factory->createStream(json_encode([
                'method' => $request->getMethod(),
                'path' => $request->getUri()->getPath(),
            ], JSON_THROW_ON_ERROR)));
    }
});

$app->listen(3000);
```

## Async PHP, backed by Tokio

Each HTTP request runs in its own root Fiber. When native work must wait, the Fiber yields a typed operation to Rust. Tokio waits for the timer, descriptor, DNS lookup, filesystem task, process, or signal while other requests continue making progress.

```php
use Pam\Filesystem\File;
use Pam\Http\Client;
use Pam\Net\Dns;
use Pam\Process\Command;
use function Pam\Async\all;
use function Pam\Async\async;
use function Pam\Async\delay;

$results = all([
    'profile' => async(static function (): string {
        delay(0.025);
        return File::read(__DIR__ . '/profile.json', maxBytes: 1_048_576);
    }),
    'addresses' => async(static fn (): array => Dns::resolve('example.com', timeout: 2.0)),
    'health' => async(static fn () => (new Client(timeout: 3.0))
        ->get('https://example.com/health')
        ->json()),
]);

$command = Command::run(
    [PHP_BINARY, '-r', 'echo hash("sha256", "pam");'],
    timeout: 2.0,
);
```

The async layer includes:

- Futures and `await`;
- timers and deadlines;
- cancellation tokens;
- bounded channels;
- Fiber-local context;
- mutexes;
- pollable stream readiness;
- TCP and verified TLS connections;
- filesystem and DNS operations;
- safe subprocess execution using argv, without a shell;
- signal watchers.

Subprocesses receive their own Unix process group. On timeout, Pam sends `TERM`, applies `KILL` after a bounded grace period, and reaps the process so descendants and zombies are not left behind.

Amp Futures can be passed to `Pam\Async\await()`. Revolt remains driven by the package's own driver for compatibility; use Pam's native operations on the hottest paths.

See [Async runtime](docs/async-runtime.md) for the execution model.

## Streaming and backpressure

Streaming responses are incremental. A bounded Rust channel sits between the PHP generator and the network transport, so a slow client cannot cause unbounded PHP-side buffering.

```php
$app->get('/events', static function ($request, $response) {
    return $response->sse((static function (): Generator {
        for ($sequence = 1; $sequence <= 10; ++$sequence) {
            yield [
                'sequence' => $sequence,
                'time' => microtime(true),
            ];

            Pam\Async\delay(0.5);
        }
    })());
});
```

`Pam\Stream\Readable`, `Writable`, and `Duplex` enforce a `highWaterMark`. Client disconnects cancel the request Fiber and execute its cleanup path.

## WebSockets on the same port

Install `pushinbr/pam-socket`; HTTP and RFC 6455 WebSockets then share the same runtime listener:

```php
use Pam\Socket\Server as SocketServer;
use Pam\WS\Socket;

$io = SocketServer::create();
$io->auth(static fn (array $context): bool =>
    ($context['headers']['authorization'] ?? '') === 'Bearer secret'
);

$io->on('connection', static function (Socket $socket) use ($io): void {
    $socket->join('lobby');
    $socket->emit('welcome', [
        'id' => $socket->id,
        'resumeToken' => $socket->resumeToken,
    ]);

    $socket->on('chat.message', static function (array $data, $ack) use ($io): void {
        $io->to('lobby')->emit('chat.message', $data);
        $ack->send(['accepted' => true]);
    });
});

$app->listen(3000);
```

The real-time layer includes:

- rooms and broadcasts;
- acknowledgements;
- text and binary frames;
- heartbeat and idle timeouts;
- bounded outbound queues and backpressure metrics;
- RFC 7692 `permessage-deflate`;
- authentication hooks;
- connection and message limits;
- HMAC-authenticated session resume tokens;
- Redis Streams and NATS adapters for multi-worker or multi-node broadcast.

The event envelope is intentionally simple:

```json
{"id":"ack-1","event":"chat.message","data":{"text":"hello"}}
```

The protocol is inspired by the ergonomics of Socket.IO, but it is **not Engine.IO**. Use a standard WebSocket client; Socket.IO clients are not wire-compatible.

## Request-scoped state and memory safety

Long-lived applications must not leak request state into the next request. Pam handles runtime-owned state and gives application code an explicit scope for everything else:

```php
use Pam\Runtime\RequestScope;

$scope = RequestScope::current();
$scope->set('tenantId', 42);

$handle = $scope->manage(fopen('/tmp/pam-app.log', 'ab'));
$scope->defer(static function (): void {
    // Roll back a transaction, release a lock, or close an external context.
});
```

Cleanup runs in LIFO order after success, exceptions, cancellation, timeout, or disconnect. Sampled leak detection compares PHP memory and resource counts before and after cleanup and exposes anomalies through diagnostics and metrics.

The memory integration test intentionally creates cyclic object graphs and abandoned Futures across **10,000 requests** after warm-up. The latest optimized validation measured a `51 MiB` RSS baseline and high-water mark with `0 MiB` sustained growth. A separate mixed Laravel/package contract held an `82 MiB` baseline, high-water mark, and final RSS across 2,000 measured requests after warm-up.

Application code is still responsible for its own persistent globals and singletons: never retain a Request, Response, authenticated user, transaction, or tenant in static state.

Laravel managers, facades, and parts of its container are process-global. The
binary-owned Laravel host therefore enforces exactly one active PHP request or
Socket callback per worker and scales with `pam start --workers N`. This is a
correctness boundary, not a tuning suggestion. Native Pam applications can keep
multiple suspended request Fibers in flight inside one worker.

## Built for production operations

Development is one process:

```bash
pam dev index.php
```

Production is a supervised cluster:

```bash
pam start index.php \
  --workers 10 \
  --max-requests 10000000 \
  --admin-address 127.0.0.1:3010
```

The production master:

- starts workers with `SO_REUSEPORT`;
- replaces crashed workers with exponential backoff;
- monitors request deadlines outside the Zend Engine;
- kills and replaces a worker stuck inside blocking PHP;
- staggers worker recycling to avoid synchronized churn;
- drains on `SIGTERM`;
- performs readiness-gated generational reload on `SIGHUP`;
- keeps the healthy generation alive when replacement boot fails;
- serves health and aggregate metrics outside PHP.

```bash
curl --fail http://127.0.0.1:3010/live
curl --fail http://127.0.0.1:3010/startup
curl --fail http://127.0.0.1:3010/ready
curl --fail http://127.0.0.1:3010/metrics
```

Do not expose the admin listener directly to the public Internet.

## TLS, HTTP/3, and security controls

```php
$app->listen(443, '0.0.0.0', [
    'tlsCert' => '/etc/pam/fullchain.pem',
    'tlsKey' => '/etc/pam/private-key.pem',
    'http3' => true,

    'maxBodyBytes' => 2 * 1024 * 1024,
    'maxHeaderBytes' => 32 * 1024,
    'maxHeaders' => 100,
    'headerReadTimeoutMs' => 10_000,
    'bodyReadTimeoutMs' => 30_000,
    'requestTimeoutMs' => 30_000,
    'maxConcurrentRequests' => 4096,
    'responseStreamQueueCapacity' => 16,
    'maxResponseBytes' => 256 * 1024 * 1024,
    'maxResponseChunkBytes' => 1024 * 1024,

    'rateLimitPerSecond' => 200,
    'corsOrigins' => ['https://app.example.com'],
    'trustedProxies' => ['127.0.0.1'],
    'exposeErrors' => false,

    'websocketMaxConnections' => 10_000,
    'websocketMaxMessageBytes' => 1024 * 1024,
    'websocketQueueCapacity' => 256,
    'websocketHeartbeatMs' => 15_000,
    'websocketTimeoutMs' => 45_000,
    'websocketCompression' => true,
    'websocketResumeSecret' => getenv('PAM_WS_RESUME_SECRET'),
    'websocketResumeTtlSeconds' => 86_400,

    'telemetryHeaders' => true,
    'accessLog' => true,
    'accessLogSampleRate' => 100,

    'gcCollectCyclesEvery' => 256,
    'gcMemCachesEvery' => 1024,
    'leakDetectionSampleRate' => 1024,
    'leakThresholdBytes' => 8 * 1024 * 1024,
]);
```

TLS uses Rustls and negotiates HTTP/2 through ALPN. With TLS and `http3` enabled, Pam also listens for QUIC on UDP at the same port and advertises `Alt-Svc` from HTTP/1.1 and HTTP/2 responses.

Defenses include bounded request and response bodies, bounded streaming chunks,
header/body deadlines, slowloris protection, token-bucket rate limiting, CORS
policy, trusted proxy controls, TLS verification in the HTTP client, redirect
credential stripping, strict response-header validation, and bounded
process/stream output. A streaming response that crosses its byte limit is failed
at the transport boundary instead of being reported as a successful complete body.

## Observability without guesswork

Enable telemetry only where you need it; the default hot path stays lean.

- Prometheus metrics for requests, errors, latency, bytes, active requests, WebSockets, backpressure, event-loop lag, RSS, PHP memory, and Fibers;
- W3C `traceparent`, request IDs, and `Server-Timing` response headers;
- sampled JSON access logs, with all 5xx responses retained;
- PSR-3 logging when `psr/log` is installed;
- OpenTelemetry context and spans when its Composer packages are installed;
- structured event ring buffer;
- heap, Fiber, connection, profiling, and tracing snapshots.

```bash
pam top http://127.0.0.1:3010 --iterations 60 --interval-ms 1000
pam diagnostics index.php
pam heap index.php
pam fibers index.php
pam connections index.php
pam profile index.php
PAM_TRACE=1 pam trace index.php
```

## CLI

```text
pam index.php [arguments...]                       run a PHP script or server
pam -r <code> [arguments...]                       execute inline PHP
pam exec index.php [arguments...]                  explicitly run a PHP script
pam composer [arguments...]                        run verified Composer inside Pam
pam dev [index.php] [arguments...]                 recursive hot reload
pam start [index.php] --workers N                  supervised production cluster
pam test [directory] [--pest|--phpunit]            test inside the Embed SAPI
pam routes [index.php]                              inspect registered routes
pam inspect [index.php]                             inspect PHP, INI, ABI, and extensions
pam diagnostics [index.php]                         complete runtime snapshot
pam heap|fibers|connections [index.php]             focused diagnostic views
pam profile|trace [index.php]                       profiling and structured events
pam top [admin URL]                                 live cluster metrics
pam doctor [directory]                              compare CLI, Embed, and Composer
pam benchmark http://host/path                      built-in HTTP benchmark
pam init [directory] --template raw|api|laravel|desktop|mobile|mobile-ui
                                                    scaffold and install a project
pam init [directory] --template api --socket        add native Socket support
pam init [directory] --no-interaction               accept the default API preset
pam build [directory] --entry index.php --output dist
```

`pam dev` watches PHP files, `.env`, `composer.json`, and `composer.lock`, while ignoring heavy/generated directories. A syntax error does not kill the watcher; fix the file and save again.

`pam doctor` compares PHP version and ABI, ZTS mode, debug mode, integer size, loaded INI files, CLI/Embed extensions, Composer autoloading, and platform requirements. Environment drift is reported instead of hidden.

## Performance

Pam is built around a short hot path: Rust handles transport and protocol work, the PHP application remains in memory, logging and telemetry are opt-in, and workers scale across CPU cores.

A local development run against a trivial in-memory endpoint produced:

```text
wrk -t4 -c1000 -d10s http://127.0.0.1:3000/

Latency       2.32ms average
Requests/sec  404,150.56
Transfer/sec  53.96MB
Workers       10
```

This is a single-machine development result, **not a universal performance claim**. Hardware, kernel settings, handler work, TLS, response size, extensions, logging, and client topology all matter. Measure your own application:

```bash
pam benchmark http://127.0.0.1:3000/api/ping \
  --requests 200000 \
  --concurrency 1000

wrk -t4 -c1000 -d30s http://127.0.0.1:3000/api/ping
```

Optimize for tail latency and stability under realistic handlers—not only maximum requests per second from an empty route.
For Laravel, use the reproducible protocol in [benchmarks/README.md](benchmarks/README.md).
Pam does not claim to beat FrankenPHP or Swoole until the same application, worker
count, hardware and response contract have been measured under that protocol.

## Build a relocatable application bundle

```bash
pam composer install --no-dev --classmap-authoritative
pam build . --entry index.php --output dist
./dist/bin/pam-run
```

The bundle contains:

```text
dist/
├── app/              application source and vendor/
├── bin/pam         optimized runtime
├── bin/pam-run     isolated launcher
├── lib/libphp*.so    exact linked PHP Embed ABI
└── manifest.json     size and SHA-256 for every packaged file
```

The builder refuses to overwrite its destination, escape the project through `..`, package unsafe symlinks, include the output recursively, or bundle a Composer project without an installed autoloader.

Bundles are relocatable, but the target still needs a compatible Linux system ABI and the native dependencies required by PHP and its enabled extensions. Use the provided `Dockerfile` when you need a controlled userspace as well.

Production assets also include:

- a multi-stage, non-root `Dockerfile` with `tini`;
- a hardened systemd unit in `packaging/pam.service`;
- x86_64 and ARM64 release workflows;
- SHA-256 release files and build attestations;
- CI, security audit, memory soak, and Valgrind workflows.

Read [Production operations](docs/production.md) before deployment.

## How it works

```text
                                  ┌──────────────────────────────┐
                                  │ master control plane         │
                                  │ health · metrics · watchdog  │
                                  └──────────────┬───────────────┘
                                                 │ supervises
                SO_REUSEPORT                     ▼
client ──► TCP / TLS / QUIC ──► worker ──────────────────────────────┐
                                  │                                  │
                                  ├─ Rust: HTTP 1/2/3 + WebSocket    │
                                  ├─ Tokio: sockets, timers, I/O     │
                                  └─ Zend Engine: loaded once        │
                                               │                     │
                                      request Fiber                  │
                                      │ PHP handler                  │
                                      │ suspend typed operation ─────┤
                                      │                              │
                                      └──────── resume with result ◄─┘
```

One Zend NTS engine belongs to one worker thread. No callback enters PHP from a foreign thread. Rust validates request and response boundaries, while the C shim keeps the native ABI intentionally narrow and versioned.

The boot sequence is:

1. initialize PHP Embed with the real process arguments;
2. discover and load Composer once;
3. load Pam's runtime modules;
4. execute the application entrypoint once to register routes and callbacks;
5. open the network listeners;
6. dispatch each request in an isolated Fiber;
7. suspend to Tokio for native waits and resume the exact same Fiber;
8. deterministically clean request state and periodically release allocator caches.

Read [Architecture](docs/architecture.md) and the versioned [Native API](docs/native-api.md) for internals.

## Validation

The repository's release gate is intentionally strict:

```bash
cargo fmt --all -- --check
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo test --locked --all-targets --all-features -- --test-threads=1

compat/composer-smoke/vendor/bin/phpstan analyse \
  -c compat/composer-smoke/phpstan.neon --no-progress
compat/laravel-smoke/vendor/bin/phpstan analyse \
  -c compat/laravel-smoke/phpstan.neon --no-progress
compat/composer-smoke/vendor/bin/phpunit \
  -c compat/composer-smoke/phpunit.xml
./target/debug/pam test compat/composer-smoke --phpunit --colors=never
./target/debug/pam test compat/composer-smoke --pest --colors=never

pam composer audit --working-dir=compat/composer-smoke --locked
pam composer audit --working-dir=compat/laravel-smoke --locked
cargo audit
```

The integration suite starts real servers and covers CLI behavior, hot reload, master/worker supervision, crash recovery, worker recycling, failed and successful reloads, watchdog replacement, REST, traditional PHP state, PSR contracts, multipart uploads, sessions, TLS, HTTP/2, HTTP/3, slowloris defense, request/response limits, CORS, rate limiting, metrics, WebSockets, fragmented NATS frames, native I/O, concurrent suspended Fibers, streaming backpressure/cancellation, Laravel binary ranges, SSE, a reentrant HTTP client, Composer packages, and memory/RSS stability.

Latest local release gate:

- **38 Rust and end-to-end tests passed**;
- **PHPStan level 9 passed with no errors**;
- **PHPUnit passed inside the Embed SAPI with 7 tests and 43 assertions; Pest passed inside Embed**;
- **Composer audit reported no known advisories in either locked contract**;
- **Cargo audit reported no known vulnerabilities across 193 locked dependencies**.

The optimized 10,000-request runtime soak measured a `51 MiB` post-warmup baseline
and high-water mark with `0 MiB` sustained growth. The mixed Laravel/package soak
remained at `82 MiB` from its post-warmup baseline through its final sample.

## Known limitations

Pam is ambitious, but the boundaries matter:

- **One PHP thread per worker.** Multiple requests can remain in flight and interleave at suspension points, but only one executes PHP bytecode at a time inside a worker. Scale CPU with `pam start --workers N`.
- **Laravel is serialized per worker.** PAM forces `maxConcurrentRequests=1` for the persistent Laravel host because framework managers and facades are process-global. Add workers for concurrency; PAM refuses an unsafe override.
- **Blocking extensions still block.** Pam does not transparently replace every `php_stream` or syscall hidden inside arbitrary extensions. Use native Pam I/O, a cooperative Composer library, `ProcessPool`, or additional workers.
- **Third-party event loops are compatibility bridges.** Amp/Revolt work through Fiber integration, but their selected driver is not transformed into Tokio. It can occupy a worker while running.
- **Direct-mode watchdog limits.** A synchronous callback can only be identified as late after it returns in direct mode. Production `start` mode enforces the deadline externally and replaces a stuck worker; the in-flight request is lost and should be retried only when safe/idempotent.
- **WebSocket reloads require reconnect.** Generational reload drains the old worker, but persistent clients must reconnect and may use `sessionId` plus `resumeToken`.
- **Socket.IO is not implemented.** Pam speaks standard RFC 6455 WebSocket, not Engine.IO.
- **HTTP/3 currently covers request/response.** WebSockets continue to use the HTTP/1.1 upgrade path; WebTransport and WebSocket over HTTP/3 are not implemented.
- **Persistent application rules apply.** Frameworks and packages must not retain request-specific state in globals or singletons.
- **Platform scope is currently Linux/Unix.** PHP 8.4 Embed must be ABI-compatible with the PHP CLI and extensions used by the application.

Experimental does not mean careless. It means the contracts are explicit, the test suite is aggressive, and production adoption should still begin with staging, soak tests, representative traffic, and a rollback plan.

## Project principles

1. **PHP should still feel like PHP.** Elegant APIs, strict types, familiar request/response semantics.
2. **Composer is non-negotiable.** No competing package manager and no ecosystem fork.
3. **Performance must be structural.** Persistent boot, bounded queues, native transport, controlled observability.
4. **Safety beats benchmark theater.** Deadlines, limits, cancellation, cleanup, audits, and honest measurements.
5. **The native boundary stays small.** Version the ABI, validate both sides, never call Zend from the wrong thread.
6. **Production is part of the runtime.** Health, metrics, reload, recovery, packaging, and diagnostics are core features.

## Contributing

Pam is early enough that excellent contributions can still shape its architecture.

Before submitting a change:

1. keep public PHP APIs small, typed, and readable;
2. preserve Composer and PSR compatibility;
3. add limits and cancellation to every potentially unbounded operation;
4. add regression coverage for the behavior;
5. run the complete validation gate above;
6. document new capabilities and their production trade-offs.

Bug reports are most useful with the Pam version, PHP Embed version, `pam doctor` output, minimal application, expected behavior, actual behavior, and a reproducible command.

## License

Pam is released under the [MIT License](LICENSE).

---

<div align="center">

**Keep the language. Keep the ecosystem. Upgrade the runtime.**

</div>
