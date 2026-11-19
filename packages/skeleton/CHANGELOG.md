# Changelog

All notable changes to the Nimbus skeleton are documented in this file.

## Unreleased

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

