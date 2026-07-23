# Pam packages and extension model

Pam deliberately separates its runtime from its application ecosystem. The `pam`
binary owns PHP Embed, Tokio, native transports, asynchronous operations, worker
lifecycle and diagnostics. Composer owns every optional application feature.

## First-party packages

| Package | Responsibility |
| --- | --- |
| `pam/core-api` | Stable contracts and runtime capability checks for package authors |
| `pam/api` | HTTP router, middleware pipeline, error boundary and provider discovery |
| `pam/socket` | Event-oriented WebSocket API, rooms, broadcasts and distributed adapters |
| `pam/psr-bridge` | PSR-7, PSR-15 and PSR-17 implementations and adapters |
| `pam/testing` | In-memory HTTP client and fluent response assertions |
| `pam/skeleton` | Minimal production-oriented project template |

Applications should depend on packages directly. There is no hidden Pam lockfile,
global package store or alternative registry.

```bash
pam composer require pam/api
pam composer require pam/socket
pam composer require --dev pam/testing
```

## Package discovery

A Composer package can register one or more providers:

```json
{
    "name": "acme/pam-health",
    "require": {
        "pam/core-api": "^0.1"
    },
    "extra": {
        "pam": {
            "providers": [
                "Acme\\Health\\HealthServiceProvider"
            ]
        }
    }
}
```

```php
use Pam\Contracts\Http\ApplicationInterface;
use Pam\Contracts\Package\ServiceProviderInterface;

final class HealthServiceProvider implements ServiceProviderInterface
{
    public function register(ApplicationInterface $application): void
    {
        $application->route('GET', '/health', static fn ($request, $response) =>
            $response->json(['status' => 'ok']));
    }

    public function boot(ApplicationInterface $application): void
    {
        // Resolve configuration or complete wiring after all providers register.
    }
}
```

`pam/api` reads `vendor/composer/installed.json`, validates every provider,
deduplicates it and writes data-only `.pam/cache/packages.json` using an atomic rename. The
cache is invalidated when Composer metadata or the lockfile changes. Set
`PAM_DISABLE_PACKAGE_DISCOVERY=1` to disable discovery and register providers
explicitly.

## Compatibility contract

Packages can call `RuntimeCompatibility::discover()->assert()` during boot and
request the native capabilities they actually need. ABI and capability values are
sequential integer enums; existing values are never repurposed.

Package code must assume that its objects may remain alive for the entire worker
lifetime. Never retain a request, response, authenticated identity, tenant,
transaction or upload in static state. Attach request-owned cleanup to
`Pam\Runtime\RequestScope`.

## Versioning and publishing

Runtime releases and first-party packages use independent semantic versions.
`pam/core-api` is the compatibility seam: packages constrain its version and check
the runtime ABI/capabilities instead of depending on Pam's Rust implementation.
Breaking PHP contract changes require a new major package version; breaking native
ownership or signatures require a new native ABI.

The package directories in this monorepo are independently valid Composer
packages. Before publishing, each one must pass `composer validate --strict`, the
locked compatibility suite, PHPStan level 9, host PHPUnit and PHPUnit/Pest inside
the Pam Embed SAPI.
