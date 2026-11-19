# Changelog

All notable changes to Nimbus are documented here. Nimbus follows Semantic
Versioning; the 0.x line may still adjust public API between minor versions.

## Unreleased

### Added
- `Input`: typed requests validated from public properties and attributes (`Min`, `Max`, `Email`,
  `Url`, `Uuid`, `Regex`, `In`, `Field`), nested inputs, enums and dates; controllers and closures
  receive them already validated, and `Input::schema()` feeds the OpenAPI document.
- `Paginator::paginate()` (numbered pages) and `Paginator::cursor()` (keyset); `JsonResource::collection()`
  adds their `meta` and `links`.
- `auth()` and `can()` on routes and route groups.
- `docs()` serves `openapi.json` and an API reference page.
- Redis stores for rate limits, idempotency and response caching over a per-command pool.
- `AsyncPostgresDriver`: with ext-pgsql, Postgres queries suspend the request Fiber instead of the
  worker (`DB_ASYNC=false` keeps blocking PDO). Requires `pushinbr/arcano` 0.2.3.
- `Hash` (Argon2id) and `Crypt` (sodium secretbox with `APP_KEY`).
- `Psr\Log\LoggerInterface` resolves to the PAM runtime structured log.

### Changed
- Each worker keeps its database connections open between requests and reuses the ORM.
- Closures declaring more than `($request, $response)` are resolved through the container.
- The container compiles parameter plans once per worker instead of reflecting per request.
- Route paths may omit the leading slash.

## 0.1.0 - 2026-11-19

Nimbus replaces `pushinbr/pam-http`, which stays frozen at 2.2.x.

### Added
- `routes.php`: `App::routes()` loads a routes file that receives the router as `$router`.
- `controller()` groups routes of one controller in a closure; each route names only the method.
- `resource()` registers index, store, show, update and destroy, and its closure adds extra
  routes bound to the same controller.

### Changed
- Package `pushinbr/pam-http` is now `pushinbr/nimbus`; namespace `Pam\Api\` is now `Nimbus\`
  and `Pam\App` is now `Nimbus\App`.
- Arcano is the only ORM; `QueryMonitor` and `DatabaseHealthCheck` read Arcano connections.

### Removed
- The third-party ORM integration and every `illuminate/*` dependency.
