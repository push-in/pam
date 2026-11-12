# Upgrading PAM API

## Compatibility policy

PAM API uses `MAJOR.MINOR.PATCH` Semantic Versioning.

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

## From Eloquent to Arcano (2.1)

The Eloquent bridge keeps working in 2.x; Arcano is the recommended ORM from
2.1. The two can run side by side on the same `DatabaseConfig` while models
move one at a time.

1. `pam composer require pushinbr/arcano` and register `ArcanoServiceProvider`
   with an `ArcanoSchemaCompiler` for the entity directory (see README).
2. Turn each Eloquent model into a plain class with `#[Entity]` and `#[Column]`
   attributes that map the existing table and column names (`table:`, `name:`).
3. Replace `Model::query()` calls in repositories with
   `ArcanoManager::select()`, `repository()` and `entityManager()->persist()`.
4. Keep running the existing Eloquent migrations until every table is mapped;
   then `ArcanoMigrationManager::generate()` must report no difference, and new
   schema changes are generated from the entities.
5. Remove `EloquentServiceProvider` and the `illuminate/*` packages when no
   model is left.

## From 1.x to 2.0

1. Upgrade the PAM runtime and PHP to supported versions before changing the
   package.
2. Replace invokable-only controller routes when a named action is clearer:

   ```php
   $app->post('/login', [LoginController::class, 'onLogin']);
   ```

3. Register Eloquent using `EloquentServiceProvider` and move database config
   to `DatabaseConfig`.
4. Keep request, auth, tenant and transaction state in scoped bindings; never
   retain them in singletons or static properties.
5. Use Form Requests for validation and Resources for domain responses.
6. Run `composer verify`, the real-network suite and the public API
   compatibility gate before deployment.

The executable `tests/fixtures/v1.0.2/application.php` preserves the route and
middleware forms published by the greatest stable 1.x release at the 2.0 cut.
`UpgradeFromV1Test` runs that application unchanged on 2.0 and binds the fixture
to the public mirror tag, commit and README digest recorded in `source.json`.
This proves the documented 1.x application path while the reflection-derived
surface gate protects the complete supported 2.x API after release.
