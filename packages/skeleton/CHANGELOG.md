# Changelog

All notable changes to the Nimbus skeleton are documented in this file.

## Unreleased

## 0.4.0 - 2026-11-24

### Added
- `bootstrap/app.php` builds the `Nimbus\App` without listening; `index.php` requires it and
  `bin/nimbus tinker` (or `composer tinker`) opens a PsySH shell over the same application.
  `psy/psysh` is a dev dependency.
- `GET /health` and `GET /ready` (database check, 503 while draining), telemetry and Sentry
  switched on by environment (`NIMBUS_METRICS`, `OTEL_*`, `SENTRY_DSN`).
- `PAM_HOST` selects the listen address (containers use `0.0.0.0`).
- Production `Dockerfile`: official PAM release, Composer install without dev packages,
  distroless non-root runtime with a health check.

### Changed
- Requires `pushinbr/nimbus` `^0.7`.

## 0.3.2 - 2026-11-21

### Changed
- Requires `pushinbr/arcano` `^0.2.8` and `pushinbr/nimbus` `^0.6`.

## 0.3.1 - 2026-11-19

### Changed
- Requires `pushinbr/arcano` `^0.2.6`.

## 0.3.0 - 2026-11-19

### Changed
- Requires `pushinbr/nimbus` `^0.5` and `pushinbr/arcano` `^0.2.5`.

## 0.2.0 - 2026-11-19

### Changed
- Requires `pushinbr/nimbus` `^0.3` and `pushinbr/arcano` `^0.2.4`.
- `tests/ApplicationTest.php` extends `Nimbus\Testing\TestCase` with `RefreshDatabase`
  (in-memory SQLite, one rolled-back transaction per test).

### Added
- `database/factories/ProductFactory.php` and `database/seeders/DatabaseSeeder.php`;
  `pam composer seed` (`bin/migrate seed`) runs the seeder. `fakerphp/faker` is a dev dependency.

## 0.1.0 - 2026-11-19

`pushinbr/pam-skeleton` is now `pushinbr/nimbus-skeleton`.

### Changed
- Requires `pushinbr/nimbus` instead of `pushinbr/pam-http`.
- Application code lives in `app/` (`App\Controllers`, `App\Requests`, `App\Resources`,
  `App\Models`, `App\Data`, ...) and every route in `routes.php`.

