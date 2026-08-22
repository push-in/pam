# Pam packages and extension model

Pam deliberately separates its runtime from its application ecosystem. The `pam`
binary owns PHP Embed, Tokio, native transports, asynchronous operations, worker
lifecycle and diagnostics. Composer owns every optional application feature.

## First-party packages

| Package | Responsibility |
| --- | --- |
| `pushinbr/pam-contracts` | Stable contracts and runtime capability checks for package authors |
| `pushinbr/pam-http` | HTTP router, middleware pipeline, error boundary and provider discovery |
| `pushinbr/pam-socket` | Event-oriented WebSocket API, rooms, broadcasts and distributed adapters |
| `pushinbr/pam-psr` | PSR-7, PSR-15 and PSR-17 implementations and adapters |
| `pushinbr/pam-testing` | In-memory HTTP client and fluent response assertions |
| `pushinbr/pam-skeleton` | Minimal production-oriented project template |

Applications should depend on packages directly. There is no hidden dependency
lockfile or alternative package format. The optional signed PAM catalog
authenticates official compatibility metadata and bytes; Composer and
`composer.lock` still own dependency resolution.

```bash
pam composer require pushinbr/pam-http
pam composer require pushinbr/pam-socket
pam composer require --dev pushinbr/pam-testing
```

## Package discovery

A Composer package can register one or more providers:

```json
{
    "name": "acme/pam-health",
    "require": {
        "pushinbr/pam-contracts": "^1.0"
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

This monorepo is the only development source for first-party packages. Read-only
distribution mirrors place each package's `composer.json` at the repository root:

| Package | Distribution mirror |
| --- | --- |
| `pushinbr/pam-contracts` | `push-in/pam-core-api` |
| `pushinbr/pam-http` | `push-in/pam-http` |
| `pushinbr/pam-socket` | `push-in/pam-socket` |
| `pushinbr/pam-psr` | `push-in/pam-psr-bridge` |
| `pushinbr/pam-testing` | `push-in/pam-testing` |
| `pushinbr/pam-skeleton` | `push-in/pam-skeleton` |

Do not commit directly to a mirror. Package changes, issues and pull requests
belong in `push-in/pam`.

Runtime tags continue to create coordinated package releases. Packages may also
advance independently after `1.x`: the **Independent Composer package release**
workflow accepts one known Composer package, a source ref already integrated
into `main`, and its own `vX.Y.Z` tag. It requires a dated package changelog,
constructs and verifies the isolated history, uploads a provenance manifest and
uses only that mirror's scoped deploy key. Re-running either release mode is
idempotent only when an existing mirror tag resolves to the exact expected split
commit.

`pam/core-api` remains the compatibility seam: packages constrain its version and
check runtime ABI capabilities instead of depending on Pam's Rust implementation.

Core API protocol 1 also defines non-HTTP transport providers for queues,
pub/sub, streams, and RPC. Providers declare integer kinds/capabilities, payload
and batch ceilings, explicit start/stop lifecycle, publish/receive operations,
and acknowledgement dispositions. See [Server transport plugins](server-transports.md).
Breaking PHP contracts require a new major package version; breaking native
ownership or signatures require a new native ABI.

Before any mirror is updated, every package must pass `composer validate --strict`,
the locked compatibility suite, PHPStan level 9, host PHPUnit and PHPUnit/Pest
inside the Pam Embed SAPI. Run the package-specific validation locally with:

```bash
scripts/package-release.sh validate
scripts/package-release.sh validate-tag v1.0.2
scripts/package-release.sh validate-package-tag pushinbr/pam-http v2.0.0
scripts/package-release.sh package-matrix pushinbr/pam-http
```

Independent releases are deliberately single-package operations. Release
dependencies in topological order (for example API before a skeleton that
requires its new major), wait for Packagist to expose each tag, and run a public
Composer dry-run before releasing the dependent package.

The workflow uses one write-enabled deploy key per mirror. Each private key is an
Actions secret scoped by the package map in `packages/packages.json`; a compromised
key therefore cannot modify another package or the monorepo. GitHub's published
SSH host keys are loaded through its authenticated API instead of accepting an
unverified host key.

After the mirrors become public, submit each repository to Packagist once and
enable its GitHub hook. Composer then installs tagged distribution archives
normally. For authenticated official capabilities, PAM exposes only the already
verified archive through an ephemeral canonical Composer artifact repository; it
does not persist a competing repository in consumer manifests.
