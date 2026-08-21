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

The 2.0 release candidate will add an executable upgrade fixture from the
greatest stable 1.x release. Until that fixture is green, this guide is a
development migration contract rather than final upgrade certification.

