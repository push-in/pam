<div align="center">

# ⚡ PAM

### PHP was never the ceiling. The runtime was.

**One persistent PHP platform for servers, Laravel, real native mobile apps, and secure desktop software.**

Rust owns the runtime. Tokio owns concurrency. PHP owns your product.

[![Status](https://img.shields.io/badge/PAM-1.0%20stable-16a34a?style=for-the-badge)](https://push-in.github.io/pam-docs/project/status/)
[![PHP](https://img.shields.io/badge/PHP-8.4-777BB4?style=for-the-badge&logo=php&logoColor=white)](https://www.php.net/)
[![Rust](https://img.shields.io/badge/Rust-1.88%2B-000000?style=for-the-badge&logo=rust&logoColor=white)](https://www.rust-lang.org/)
[![License](https://img.shields.io/badge/License-Apache%202.0-2563eb?style=for-the-badge)](LICENSE)

**[Read the docs](https://push-in.github.io/pam-docs/introduction/) · [Install PAM](https://push-in.github.io/pam-docs/getting-started/installation/) · [Create your first app](https://push-in.github.io/pam-docs/getting-started/first-app/) · [Explore PAM Native](https://push-in.github.io/pam-docs/native/overview/)**

</div>

---

PAM — **PHP, Always in Memory** — is an unapologetically ambitious answer to a
simple question: **how far can PHP go when we stop rebuilding and discarding it
on every request?**

Much further.

PAM keeps the Zend Engine and your Composer application alive inside a
supervised, event-driven runtime powered by Rust, Tokio, and PHP Fibers. That
same foundation can serve HTTP and WebSockets, run Laravel with isolated request
sandboxes, render actual Android Views and UIKit controls, and power desktop
applications inside capability-secured native windows.

This is not PHP imitating another ecosystem. **This is PHP with a modern systems
boundary built around its strengths.**

> [!IMPORTANT]
> PAM 1.0 stabilizes the documented CLI, server runtime, Composer packages,
> editor tooling, Android distribution, and generated iOS host contracts. Read
> the [project status](https://push-in.github.io/pam-docs/project/status/) and
> [known limitations](#known-limitations), then validate your own extensions,
> devices, credentials, and workloads before production deployment.

## One command. Four product surfaces. No ecosystem reset.

| Build | What PAM changes | What you keep |
| --- | --- | --- |
| **PAM Server** | Persistent Zend, native HTTP/WebSockets, async I/O, supervised workers | PHP, Composer, PSRs, your application |
| **Laravel on PAM** | Boot once, isolate every request, add native operations and observability | Laravel, Artisan, packages, conventions |
| **PAM Native** | Reconcile in Rust and render real Android/iOS controls | Reactive PHP components, state, routes, Composer |
| **PAM Desktop** | Servo UI, Rust process control, explicit native capabilities | PHP application logic, HTML/CSS/JS views |

Most platforms ask you to choose between the PHP ecosystem and a modern runtime.
PAM rejects the trade-off. Composer stays Composer. Laravel stays Laravel. Rust
handles transport, scheduling, reconciliation, supervision, and the native edge;
PHP remains the expressive product layer your team already knows.

## Your first persistent application in 60 seconds

No global PHP. No global Composer. No FPM pool. No Rust toolchain for application
developers. Install one verified runtime and create the product you want:

```console
$ curl --proto '=https' --proto-redir '=https' --tlsv1.2 \
    --connect-timeout 15 --max-time 60 --max-filesize 1048576 -fsSL \
    https://github.com/push-in/pam/releases/latest/download/install.sh | sh

$ pam init my-api --template api
✓ Project created
✓ Composer dependencies installed
✓ PAM runtime ready

$ cd my-api && pam dev
⚡ PAM listening on http://127.0.0.1:3000
↻ Hot reload enabled

$ curl http://127.0.0.1:3000/api/ping
{"message":"pong"}
```

Your application is now warm and persistent. Composer loaded once. The runtime
stays alive. Requests execute in isolated Fibers.

Choose another target without learning another platform CLI:

```bash
pam init my-laravel-app --template laravel  # Persistent Laravel
pam init my-native-app  --template mobile   # Android + iOS
pam init my-desktop-app --template desktop  # Linux + macOS + Windows
pam init my-product     --template product  # Server + Native + Desktop
```

The `product` preset creates one coordinated workspace—not three unrelated
examples. It places the PAM API in `apps/server`, the Android/iOS application
in `apps/native`, the Desktop application in `apps/desktop`, and a versioned
integer-backed PHP contract plus shared light/dark design tokens in
`packages/contracts`. The generated vertical slice includes a bounded offline
check-in queue, authenticated Desktop worker commands, cross-surface status,
tests, cleanup rules, and a deterministic release manifest. Use `--no-install`
to generate it without downloading dependencies.

Inside every PAM project, the workflow is the same:

```bash
pam doctor
pam support --output pam-support.json
pam support --manager --output pam-support-manager.json
pam dev
pam test
pam build
pam package
```

When a development or CI environment needs investigation, `pam support` emits a
bounded JSON report to standard output without copying source files, environment
variables, or network data. Paths are redacted, persistence is explicit, and an
existing report is never overwritten. See the [CLI guide](docs/cli.md#privacy-safe-support-reports).
Add `--manager` to include a bounded, separately hashed and path-redacted process
health/resource snapshot; log contents remain excluded.

## Native means native. Reactive means reactive.

PAM Native does not ship a DOM, a browser pretending to be an app, or a
JavaScript runtime between your state and the platform. PHP describes the UI;
Rust validates, diffs, and lays it out; Kotlin and Swift commit bounded mutations
to real native controls on the UI thread.

Write a reactive component in PHP:

```php
<?php

use Pam\Native\Attributes\State;
use Pam\Native\Component;

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
        <Text class="eyebrow">LIVE NATIVE STATE</Text>
        <Text class="value">{{ $count }}</Text>
        <Button label="Increment" @press="increment" />
    </Column>
</template>

<style scoped>
    .counter { padding: 24px; gap: 16px; align-items: center; }
    .eyebrow { font-size: 12px; font-weight: 700; }
    .value { font-size: 56px; font-weight: 800; }
</style>
```

Tap the native button. PAM dispatches the event to the persistent PHP component,
updates `#[State]`, renders the next typed tree, calculates the minimal diff in
Rust, and commits only the necessary mutation to Android or iOS.

```text
native event → PHP action → reactive state → Rust diff → UI-thread commit
```

The result is a product-grade native foundation:

- real Android Views and UIKit controls;
- native stacks, tabs, drawers, sheets, headers, gestures, and transitions;
- recycled lists and grids built for large datasets;
- navigation, scroll, and focused-input preservation during hot reload where supported;
- native animations and gestures without per-frame traffic through PHP;
- camera, media, files, SQLite, secure storage, notifications, location, sensors,
  background work, sharing, widgets, App Intents, and Live Activities;
- a plugin SDK for product-specific Kotlin and Swift modules or views;
- bounded queues, payloads, caches, restoration, diagnostics, profiling, and
  repeatable performance budgets.

Start it with the same CLI:

```console
$ pam init orbit --template mobile
$ cd orbit
$ pam doctor --fix
$ pam dev
✓ PHP component runtime started
✓ Native host connected
⚡ Hot reload ready
```

**Go deeper:** [Native overview](https://push-in.github.io/pam-docs/native/overview/) ·
[components](https://push-in.github.io/pam-docs/native/components/) ·
[state and lifecycle](https://push-in.github.io/pam-docs/native/state-and-lifecycle/) ·
[navigation](https://push-in.github.io/pam-docs/native/navigation/) ·
[hot reload](https://push-in.github.io/pam-docs/native/hot-reload/) ·
[plugin SDK](https://push-in.github.io/pam-docs/native/plugins/)

## Desktop without ambient superpowers

PAM Desktop combines typed PHP application logic, Rust process supervision, and
local HTML/CSS/JavaScript rendered by Servo inside native windows. It supports
multiple windows, bidirectional commands and events, background jobs, hot reload,
crash recovery, signed updates, rollback, and native distribution.

Power is explicit. Grant only what the application needs:

```php
$app->capabilities(
    Capabilities::none()
        ->filesystem(FileSystemRoot::readWrite('workspace', __DIR__.'/storage'))
        ->dialogs()
        ->clipboard()
        ->notifications()
        ->dragAndDrop(),
);

$app->command('greet', static fn (CommandContext $command): CommandResult =>
    CommandResult::success([
        'message' => 'Hello, '.$command->string('name', 'world').'.',
    ]),
);
```

Browser code can invoke only registered commands. Filesystem access is limited
to named roots. The local bridge uses a random loopback port, a cryptographically
random per-process token, matching-origin checks, and a restrictive CSP.

```console
$ pam init studio --template desktop
$ cd studio
$ pam desktop doctor
$ pam desktop dev
$ pam desktop build
✓ Application bundle created
✓ SHA-256 integrity manifest written
```

Build self-contained packages for Linux, macOS, or Windows; create DEB, DMG, or
MSIX output on the corresponding native host; and keep feed signing plus updates
behind explicit PHP policy and a pinned Ed25519 public key.

> [!NOTE]
> PAM Desktop 1.2 is alpha software and pins a Servo LTS release. It is ready
> for prototypes and controlled applications, but does not claim Electron
> feature parity yet. Release evidence records the exact engine revision instead
> of presenting that moving implementation detail as the product version.

**Explore Desktop:** [overview](https://push-in.github.io/pam-docs/desktop/overview/) ·
[windows and commands](https://push-in.github.io/pam-docs/desktop/windows-and-commands/) ·
[native capabilities](https://push-in.github.io/pam-docs/desktop/capabilities/) ·
[security](https://push-in.github.io/pam-docs/desktop/security/) ·
[distribution](https://push-in.github.io/pam-docs/desktop/distribution/)

## Laravel, still unmistakably Laravel

```console
$ pam init my-laravel-app --template laravel
$ cd my-laravel-app
$ pam dev pam.php
⚡ Laravel booted once and listening on http://127.0.0.1:3000
```

PAM downloads the official skeleton, installs normal Composer dependencies,
runs package discovery, generates the application key, and keeps Laravel intact
in `vendor`. Each request receives an isolated application sandbox while the
framework remains warm.

Artisan remains Artisan:

```bash
pam artisan migrate
pam artisan route:list
pam artisan test
pam artisan queue:work
```

The executable compatibility matrix covers Laravel 12 and 13 with SQLite,
MySQL, PostgreSQL, Redis, database queues, Artisan, Sanctum, Scout, Livewire,
Inertia, Reverb, Telescope, and Pulse. See the
[Laravel documentation](https://push-in.github.io/pam-docs/laravel/overview/)
for the lifecycle, package matrix, observability, Cloud, Forge, autoscaling, and
deployment contracts.

Existing Laravel Octane applications can keep Octane's lifecycle and use PAM's
Rust/Tokio transport through the optional bridge:

```bash
pam composer require laravel/octane pushinbr/pam-octane
pam octane:start
```

PAM Octane supports PHP 8.4 with Laravel 12 or 13 and Octane 2.19+. Start with
the [package guide](packages/octane/README.md), then review the
[production](docs/production.md) and [security](docs/octane-security.md)
contracts before deployment.

## The runtime beneath all of it

```text
Traditional PHP                         PAM

request                                 process starts
  ├─ bootstrap PHP                        ├─ start Zend + Tokio
  ├─ load Composer                        ├─ load Composer
  ├─ build application                    ├─ build application once
  ├─ execute handler                    request 1 ─┐
  └─ discard everything                 request 2 ─┼─ isolated Fibers
request                                   request N ─┘
  └─ repeat all of the above              process stays alive
```

PAM is **not** a new language, a PHP fork, a framework, or a Composer
replacement. It is the systems layer beneath your application:

- persistent Zend Engine through the official Embed SAPI;
- Tokio scheduling, native async I/O, streaming, and backpressure;
- HTTP/1.1, HTTP/2, HTTP/3, and RFC 6455 WebSockets on the same runtime;
- supervised master/worker processes, crash recovery, graceful drain, worker
  recycling, and generational reload;
- Prometheus metrics, health endpoints, structured logs, tracing, profiling,
  diagnostics, and live `pam top`;
- verified TLS, timeouts, request limits, slowloris protection, CORS, rate
  limiting, and trusted-proxy controls;
- relocatable bundles containing the application, `vendor`, runtime, exact
  `libphp`, and a SHA-256 manifest.

## Start exploring

| I want to… | Start here |
| --- | --- |
| Understand the whole platform | **[Introduction](https://push-in.github.io/pam-docs/introduction/)** |
| Install PAM | [Installation](https://push-in.github.io/pam-docs/getting-started/installation/) |
| Build my first application | [Create your first app](https://push-in.github.io/pam-docs/getting-started/first-app/) |
| Choose Server, Laravel, Native, or Desktop | [Choose a target](https://push-in.github.io/pam-docs/getting-started/choose-a-target/) |
| Learn every command | [CLI and project console](https://push-in.github.io/pam-docs/getting-started/cli/) · [repository CLI reference](docs/cli-reference.md) |
| Understand the internals | [Architecture](docs/architecture.md) · [How it works](#how-it-works) |
| Operate PAM in production | [Production](#built-for-production-operations) · [official production guide](https://push-in.github.io/pam-docs/runtime/production/) |
| Run Laravel Octane | [PAM Octane](packages/octane/README.md) · [release checklist](docs/octane-release-checklist.md) |
| Inspect maturity and support | [Project status](https://push-in.github.io/pam-docs/project/status/) · [Known limitations](#known-limitations) |

The sections below are the deep technical reference: packages, APIs, Composer,
PSRs, async I/O, WebSockets, production, performance, architecture, and
validation.

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
├── pushinbr/pam-octane       Laravel Octane bridge
├── pam/desktop      desktop application model (separate repository)
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

Composer remains the dependency resolver and PAM does not introduce a competing
package format. Projects may opt into PAM's signed compatibility catalog to
authenticate official versions and artifact bytes before Composer sees them.

```bash
pam composer require guzzlehttp/guzzle
pam composer install
pam doctor
pam test
```

Pam discovers the nearest `composer.json`, respects `config.vendor-dir`, and loads the normal Composer autoloader. Your lockfile remains the dependency source of truth; the signed PAM catalog is an authenticity and compatibility gate, not a second solver.

Registry operators and CI can export one verified Server/Native/Desktop view with
`pam registry matrix --root root.json --root-sha256 <hex> --catalog catalog.json
--native-protocol <n> --desktop-protocol <n> --json`. The output uses stable
integer surface and result codes and is suitable as the source for compatibility
dashboards; see the [registry operations runbook](docs/registry-operations.md).
`pam registry dashboard` turns the same verified inputs into a dependency-free,
accessible static site plus its exact JSON evidence. It refuses to overwrite an
existing output directory so publication can use an atomic directory swap.

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

The listener is disabled unless `--admin-address` is present. Non-loopback
addresses require a 32–256 character `PAM_ADMIN_TOKEN` or a bounded regular
secret file selected by `PAM_ADMIN_TOKEN_FILE`; all endpoints then
require `Authorization: Bearer`, and `pam top` forwards the token automatically.
The master removes it from PHP worker environments. Do not expose the admin
listener directly to the public Internet.

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
pam up [index.php] --name api --workers N          start and detach a managed application
pam up ... [--restart-delay-ms N] [--restart-backoff-max-ms N]
           [--max-unstable-restarts N] [--min-uptime-ms N] [--no-autorestart]
pam ps                                              list managed applications
pam reload api                                      zero-downtime generational reload
pam logs api --errors --lines 200                   inspect retained manager logs
pam logs api --both --follow                        follow stdout and stderr across rotation
pam logs api --both --include-rotated --query ERROR --lines 500 --json
pam daemon start|status|stop                        control the private per-user supervisor
pam scale api 8                                     persist and apply a new worker count
pam save && pam resurrect                           save and restore the desired process list
pam startup --print|--install                       configure the systemd user service
pam monit [--json]                                  inspect process health and capacity
pam monit:history [name] [--json]                   inspect bounded one-minute history
pam dashboard [pam-dashboard.html]                  create a private static health snapshot
pam dashboard:start --token-file TOKEN              start authenticated live local view
pam dashboard:status|dashboard:stop                 inspect or stop the live view
pam config:check [pam.toml] [--json]                validate declarative multi-service config
pam apply [pam.toml] [--json]                       reconcile all declared applications
pam up ... [--memory-warning-bytes N] [--task-warning-count N]
           [--memory-max-bytes N] [--task-max-count N]  enforce cgroup-v2 limits
pam deploy api /srv/api/releases/2026-08-21         activate a readiness-gated release
pam deploy:history api [--json]                     inspect bounded release history
pam rollback api [--steps N]                        restore a previous healthy release
pam traffic:start edge --listen IP:PORT --stable IP:PORT [--tls-cert FILE --tls-key FILE]
pam traffic:set edge --candidate IP:PORT --weight-bps N
pam traffic:evaluate edge --min-candidate-requests N --max-candidate-error-bps N
pam traffic:status edge [--json]                     inspect per-version rollout evidence
pam traffic:abort|traffic:promote edge               atomically finish a rollout
pam test [directory] [--pest|--phpunit]            test inside the Embed SAPI
pam routes [index.php]                              inspect registered routes
pam inspect [index.php]                             inspect PHP, INI, ABI, and extensions
pam diagnostics [index.php]                         complete runtime snapshot
pam heap|fibers|connections [index.php]             focused diagnostic views
pam profile|trace [index.php]                       profiling and structured events
pam top [admin URL]                                 live cluster metrics
pam doctor [directory]                              compare CLI, Embed, and Composer
pam doctor --json|--schema                          emit diagnostics or its embedded contract
pam doctor --validate doctor-report.json            verify a saved report offline
pam benchmark http://host/path                      built-in HTTP benchmark
pam init [directory] --template raw|api|laravel|desktop|mobile|mobile-ui|product
                                                    scaffold and install a project
pam init [directory] --template api --socket        add native Socket support
pam init [directory] --no-interaction               accept the default API preset
pam build [directory] --entry index.php --output dist
```

`pam dev` watches PHP files, `.env`, `composer.json`, and `composer.lock`, while ignoring heavy/generated directories. A syntax error does not kill the watcher; fix the file and save again.

`pam doctor` compares PHP version and ABI, ZTS mode, debug mode, integer size, loaded INI files, CLI/Embed extensions, Composer autoloading, and platform requirements. Environment drift is reported instead of hidden.
`pam doctor --schema` prints the strict, versioned JSON Schema used by automation without requiring a network connection.
`pam doctor --validate` applies bounded structural and semantic validation to a saved report and rejects unknown fields, inconsistent codes, oversized inputs, and symlinks.

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
pam build --entry index.php --output dist
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
composer --working-dir=packages/octane verify
scripts/package-release.sh validate
```

The integration suite starts real servers and covers CLI behavior, hot reload, master/worker supervision, crash recovery, worker recycling, failed and successful reloads, watchdog replacement, REST, traditional PHP state, PSR contracts, multipart uploads, sessions, TLS, HTTP/2, HTTP/3, slowloris defense, request/response limits, CORS, rate limiting, metrics, WebSockets, fragmented NATS frames, native I/O, concurrent suspended Fibers, streaming backpressure/cancellation, Laravel binary ranges, SSE, a reentrant HTTP client, Composer packages, and memory/RSS stability.

Release evidence is generated by CI and the checked-in benchmark protocol. Do
not copy historical test counts, audit totals or soak results into a new release;
publish the output for the exact clean commit being tagged.

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
- **Host scope is Linux and macOS.** Windows users can use WSL for server projects; native Windows host binaries are not part of 1.0.

Stable contracts do not remove application-specific risk. Production adoption
should still begin with staging, soak tests, representative traffic and a tested
rollback plan.

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

See [CONTRIBUTING.md](CONTRIBUTING.md) for the development workflow,
[GOVERNANCE.md](GOVERNANCE.md) for decision-making and
[ROADMAP.md](ROADMAP.md) for current direction. Usage support is described in
[SUPPORT.md](SUPPORT.md); vulnerabilities follow [SECURITY.md](SECURITY.md).

## License

PAM is free and open-source software under the [Apache License 2.0](LICENSE).
You may use, modify, and distribute it—including commercially—subject to the
license terms. See the plain-language [licensing guide](LICENSING.md).

---

<div align="center">

**Keep the language. Keep the ecosystem. Upgrade the runtime.**

</div>
