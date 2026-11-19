# Changelog

All notable changes to the Nimbus skeleton are documented in this file.

## Unreleased

### Changed
- Requires `pushinbr/nimbus` 0.3: `tests/ApplicationTest.php` extends `Nimbus\Testing\TestCase`
  with `RefreshDatabase` (in-memory SQLite, one rolled-back transaction per test).

### Added
- `database/factories/ProductFactory.php` and `database/seeders/DatabaseSeeder.php`;
  `pam composer seed` (`bin/migrate seed`) runs the seeder. `fakerphp/faker` is a dev dependency.

## 0.1.0 - 2026-11-19

`pushinbr/pam-skeleton` is now `pushinbr/nimbus-skeleton`.

### Changed
- Requires `pushinbr/nimbus` instead of `pushinbr/pam-http`.
- Application code lives in `app/` (`App\Controllers`, `App\Requests`, `App\Resources`,
  `App\Models`, `App\Data`, ...) and every route in `routes.php`.

