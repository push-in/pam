# Upgrading Nimbus

## Compatibility policy

Nimbus uses `MAJOR.MINOR.PATCH` Semantic Versioning.

- Patch releases fix defects without intentionally breaking public contracts.
- Minor releases add compatible public API and may deprecate existing API.
- Major releases may remove APIs only after the documented deprecation window.
- Classes and methods marked `@internal` are excluded from the public BC
  promise but remain covered by tests for the release that contains them.

Deprecations must identify the replacement, first deprecated version and
earliest removal version. A removal requires an upgrade recipe and automated
compatibility evidence.

`composer api:compat` compares the reflection-derived public surface against
`api-surface.json`. Unreviewed additions fail as baseline drift; breaking
symbol, member, signature, inheritance, interface and enum changes fail as
incompatibilities. All results use sequential integer change codes. Refresh the
baseline after approving compatible additions. Refreshing it for a breaking
change is allowed only on an intentional major-version branch after documenting
every reported break; normal `composer verify` never regenerates it.

## From pam-http 2.2 to Nimbus 0.1

1. `pam composer remove pushinbr/pam-http` and `pam composer require pushinbr/nimbus`.
2. Replace namespaces: `Pam\Api\` becomes `Nimbus\` and `Pam\App` becomes `Nimbus\App`.
   Runtime classes keep their names (`Pam\Http\Request`, `Pam\Http\Response`).
3. Move route declarations to `routes.php` and load it with `$app->routes(__DIR__ . '/routes.php')`.
4. Use Arcano for the database (`ArcanoServiceProvider`); there is no other ORM integration.
