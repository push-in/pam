# Laravel on Pam

Pam runs an unmodified Laravel application in a persistent PHP Embed worker. The
integration classes live in the Pam binary under `Pam\Laravel`; the project only
depends on Laravel and its ordinary Composer packages.

## Executable compatibility matrix

Compatibility is a tested contract, not a claim based only on successful boot.
The dedicated workflow runs the same PAM binary against Laravel 11, 12 and 13 on
PHP 8.4 Embed. Laravel 12 and 13 are the maintained production matrix. Laravel
11 remains a legacy regression contract, but its
[official security support](https://laravel.com/docs/12.x/releases#support-policy)
ended on March 12, 2026 and it must not be selected for a new production deployment.
The matrix exercises:

- HTTP kernel boot, providers, middleware, routing, validation, exceptions and
  terminating callbacks;
- Eloquent transactions and events on SQLite, MySQL and PostgreSQL;
- Redis and array cache stores, database queues, sync jobs, Artisan and scheduler
  discovery;
- encrypted cookies, CSRF, isolated sessions, authentication guards and Sanctum;
- multipart uploads, Flysystem, Blade, Livewire and Inertia;
- Scout, Reverb provider discovery, and Telescope/Pulse actively persisting
  records after real HTTP requests;
- streamed and binary responses, `HEAD`, byte ranges, backpressure,
  disconnection cancellation and response limits;
- request-scoped dependency injection through Events and Bus, locale reset,
  facade reset, scoped bindings and stable RSS.

The locked Laravel 13 contract installs Inertia, Livewire, Pulse, Reverb,
Sanctum, Scout and Telescope. Horizon and Socialite are additionally exercised
on Laravel 11/12 because their current stable Composer constraints do not accept
Laravel 13; PAM does not falsify or bypass upstream package constraints.

This matrix defines the technically supported surface. No runtime can truthfully guarantee
every future Laravel release, third-party package, PHP extension and application
singleton without executing that combination. Add production-specific packages
and extensions to the contract before deployment, and use only framework versions
that still receive upstream security fixes.

## Create and run

```bash
pam init my-app --template laravel
cd my-app
pam dev pam.php
curl http://127.0.0.1:3000/api/ping
```

The initializer performs the equivalent of an official Laravel `create-project`,
then adds `pam.php`, `routes/api.php`, the API route registration and Composer
scripts named `pam:dev` and `pam:start`. Use `--socket` to register Pam's native
WebSocket transport on the same listener.

Composer runs through the Embed SAPI:

```bash
pam composer install
pam composer require laravel/sanctum
pam composer update
```

Pam caches a verified Composer PHAR under the user's XDG cache. `PAM_COMPOSER`
may point to a specific trusted PHAR. The first automatic download verifies the
official SHA-384 installer signature before executing it.

## Artisan and console processes

Pam gives Artisan an explicit CLI lifecycle even though HTTP workers use the
truthful `embed` SAPI. Arguments, standard streams, exit codes, `PHP_BINARY`,
`APP_RUNNING_IN_CONSOLE`, and console package discovery remain available:

```bash
pam artisan migrate
pam artisan route:list
pam artisan test
pam artisan queue:work
pam artisan schedule:work
```

Queue workers, Horizon, and the scheduler are independent long-running console
processes. Supervise them separately from `pam start pam.php`; they must not share
the persistent HTTP worker process.

When a hardened service has a read-only application directory, set
`PAM_LARAVEL_STORAGE_PATH` to a writable directory. Pam applies it through
Laravel's `useStoragePath()` before the kernel boots and creates Laravel's
standard `app`, `framework/cache`, `framework/sessions`, `framework/views` and
`logs` subdirectories when absent. The packaged systemd unit uses `/var/lib/pam`,
provisioned by `StateDirectory=pam`.

## Request lifecycle

The worker boots `bootstrap/app.php`, Laravel's HTTP kernel, configuration,
providers and routes once. For each request Pam:

1. creates an Illuminate request from the native request and isolated superglobals;
2. clones the booted application container into a request sandbox;
3. points the kernel, router, events, Bus pipeline, Eloquent, managers, facades,
   validator/translator and scoped bindings at that sandbox;
4. handles the Laravel request and converts status, repeated headers, cookies and body;
5. streams `StreamedResponse` and `BinaryFileResponse` incrementally through the bounded native queue, including `Range` and `HEAD` semantics;
6. terminates the Laravel request after the response finishes or is cancelled;
7. flushes request facades, controllers, auth guards, queued cookies, views,
   query logs, log context, locale-sensitive services and scoped bindings;
8. destroys the sandbox and restores the root application.

This model protects framework state between requests. Application code must still
avoid retaining a request, user, tenant or response in global/static state. Worker
recycling remains a production safety boundary for third-party packages.

Laravel's managers, facades, router and several third-party packages still contain
process-global mutable state. PAM therefore enforces `maxConcurrentRequests=1`
for this host and applies the same execution slot to Socket callbacks. An unsafe
override is rejected during startup. Scale with multiple supervised workers:

```bash
pam start pam.php --workers "$(nproc)"
```

This restriction does not apply to a native PAM application, where isolated
request Fibers may remain suspended concurrently.

Streamed and binary responses are not accumulated in PHP memory. Output is split
into 64 KiB chunks, the Rust queue applies client backpressure, disconnects cancel
the request Fiber, and `maxResponseBytes`/`maxResponseChunkBytes` enforce hard
transport boundaries. The generated `pam.php` includes production-safe defaults.

## Development reload

`pam dev pam.php` watches PHP source, routes, configuration, `.env` and Composer
metadata. Generated directories including `vendor`, `storage` and
`bootstrap/cache` are ignored. An invalid save may stop the child, but the watcher
stays alive and starts a fresh worker after the next valid change.

## Production

```bash
pam composer install --no-dev --classmap-authoritative
pam start pam.php \
  --workers 10 \
  --max-requests 100000 \
  --admin-address 127.0.0.1:3010
```

Start with roughly one worker per available CPU core, then tune from p95/p99
latency, RSS and database capacity. Laravel is synchronous unless application I/O
uses a Pam-cooperative API, so extra workers are important for blocking database
drivers and extensions.

Run the executable compatibility and memory contract before a release:

```bash
pam composer install --working-dir=compat/laravel-smoke
cargo test --test laravel -- --nocapture
compat/laravel-smoke/vendor/bin/phpstan analyse \
  -c compat/laravel-smoke/phpstan.neon --no-progress
```

See [the benchmark protocol](../benchmarks/README.md) before comparing runtimes.
