# PAM runtime and Composer ecosystem

PAM is the persistent PHP runtime. It owns only the boundaries that require the
embedded Zend Engine, Rust/Tokio, operating-system integration, process
supervision, request isolation, and the low-level Request/Response transport.

Application frameworks, integrations, hosts, and product capabilities are
Composer packages. Composer is the only dependency authority: `composer.json`
and `composer.lock` remain canonical, and PAM does not maintain a parallel
package graph or lockfile.

```text
applications
    ↓
Composer packages (HTTP, Laravel, Native, Desktop, community packages)
    ↓
PAM contracts and low-level PHP primitives
    ↓
PAM runtime (Zend + Rust/Tokio)
    ↓
operating system
```

## Permanent boundary

A capability belongs in the runtime only when it must control the Zend Engine,
participate in Fiber suspension or request isolation, integrate directly with
Tokio/the operating system, or cannot be implemented safely in PHP. Routing,
middleware, validation, authentication, framework integration, UI, storage,
sync, and vendor services belong in Composer packages.

The runtime must not contain a package-name allowlist. Official and community
packages extend the CLI and application through the same public metadata.

## Official package names

Pushin remains the Packagist vendor. Official packages use one predictable
namespace, `pushinbr/pam-*`:

| Package | Responsibility |
| --- | --- |
| `pushinbr/pam-contracts` | Stable extension contracts |
| `pushinbr/pam-http` | Routing, middleware, controllers, validation and resources |
| `pushinbr/pam-socket` | High-level WebSocket events and rooms |
| `pushinbr/pam-http-psr` | PSR-7, PSR-15 and PSR-17 HTTP interoperability |
| `pushinbr/pam-http-testing` | HTTP application testing utilities |
| `pushinbr/pam-laravel` | Laravel integration |
| `pushinbr/pam-native` | Android/iOS host and PHP application framework |
| `pushinbr/pam-desktop` | Desktop host and PHP application framework |

Surface capabilities append a functional suffix, for example
`pushinbr/pam-native-auth`, `pushinbr/pam-native-sync`, and
`pushinbr/pam-native-sync-laravel`. Names describe responsibility; they do not
create another dependency system.

## Package commands

Any Composer package may register bounded CLI commands in `extra.pam.commands`:

```json
{
  "type": "pam-extension",
  "extra": {
    "pam": {
      "commands": {
        "acme:import": {
          "script": "bin/import.php",
          "environment": {
            "ACME_CLI_MODE": "1"
          },
          "description": "Import Acme records"
        }
      }
    }
  }
}
```

PHP tools use `script`; native package CLIs use `bin`:

```json
"native:build": {
  "bin": "bin/pam-native",
  "arguments": ["build"],
  "description": "Build the native application"
}
```

`arguments` contains at most 32 validated arguments and is prepended to the
arguments supplied by the user. `environment` contains at most 32 bounded,
uppercase environment entries applied before the command starts. PAM reads
Composer's canonical `install-path` metadata, confines both target
types to the project, rejects duplicates and built-in shadowing, lists them
through `pam commands`, and adds them to generated shell completion. PHP
scripts execute in PAM's embedded PHP; binaries receive the arguments, project
working directory, and `PAM_BINARY`. Package commands have the same technical
rights whether their package is official or community maintained.

Product-level contextual commands such as `dev`, `build`, `package`,
`desktop`, and `mobile` may be supplied by an installed package and take
precedence over generic runtime commands when explicitly allowed. There are no
compiled Desktop, Native, Octane, or Laravel command adapters. Core
runtime authority—including `start`, process supervision, `composer`, `exec`,
and self-update—cannot be shadowed.

## Compatibility policy

The [package naming policy](package-naming-policy.md) defines permanent product-family ownership
and the required metapackage migration sequence.

Package and protocol evolution is additive within a major version. Renamed
packages retain a Composer compatibility package at the old name that depends on
the replacement and is marked abandoned in favor of it. Public PHP namespaces
do not change merely to mirror a distribution rename.

PHP 8.5 is the default for generated projects and bundled runtime selection.
PHP 8.4 may remain a tested compatibility line while it is supported; default
does not mean exclusive. Selection and compatibility are separate contracts.
See the [PHP version policy](php-version-policy.md).

## Renamed-package migration

Old names remain installable as Composer metapackages and depend on their
replacement. They contain no duplicate implementation:

| Compatibility name | Canonical package |
| --- | --- |
| `pushinbr/pam-api` | `pushinbr/pam-http` |
| `pushinbr/pam-core-api` | `pushinbr/pam-contracts` |
| `pushinbr/pam-psr-bridge` | `pushinbr/pam-http-psr` |
| `pushinbr/pam-psr` | `pushinbr/pam-http-psr` |
| `pushinbr/pam-testing` | `pushinbr/pam-http-testing` |
| `pushinbr/pam-mobile-ui` | `pushinbr/pam-native-ui` |
| `pushinbr/pam-native-laravel-sync` | `pushinbr/pam-native-sync-laravel` |

Existing locks continue resolving safely. Applications should replace the old
root requirement explicitly with `pam composer remove OLD` followed by
`pam composer require NEW`.
