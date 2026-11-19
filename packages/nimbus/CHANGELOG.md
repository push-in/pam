# Changelog

All notable changes to Nimbus are documented here. Nimbus follows Semantic
Versioning; the 0.x line may still adjust public API between minor versions.

## Unreleased

### Added
- `Storage`: named disks over league/flysystem v3 (local and S3, optional dependencies) with
  `StorageConfig::fromEnvironment()`, `StorageServiceProvider`, `App::storage()` and an
  injectable `Disk` (put, get, streams, exists, delete, size, mimeType, url, temporaryUrl).
- `Disk::temporaryUploadUrl()`: direct uploads by presigned S3 `PutObject`, or for local disks
  through opt-in signed `PUT {routes}/upload` and `GET {routes}/download` routes that enforce
  the signed size limit and content type.
- `UrlSigner` (HMAC-SHA256 with a key derived from `APP_KEY`, expiry, constant-time check) and
  `signed()` on routes and route groups.
- Problem codes `InvalidSignature`, `PayloadTooLarge` and `UnsupportedMediaType`.
- `Crypt::keyFromEnvironment()`.
- `RedisJobQueue`: a durable queue over `Nimbus\Redis\Connection`, with delayed jobs in a sorted set,
  lease-based reservations that return to the queue when a worker dies, and every transition in one
  Lua script; keys share a `{queue}` hash tag for Redis Cluster.
- `DatabaseJobQueue` over Arcano (`jobs` and `failed_jobs`, from `QueueTablesMigration`): reservations use
  `FOR UPDATE SKIP LOCKED` on Postgres and MySQL and a compare-and-swap update elsewhere.
- Failed-jobs store on both drivers (`FailedJobRepository`: `failed()`, `retryFailed()`, `forgetFailed()`);
  exhausted, dead-lettered and undecodable jobs land there with the exception class only.
- `RetryPolicy` and `ExponentialBackoff` (doubling delays with equal jitter); `JobWorker` takes one through
  `retryPolicy` and drains a bounded batch with `work()`. `InvokableJobHandler` runs a job's `handle()` with
  container injection.
- Scheduler (`Schedule`, `ScheduledTask`, `Scheduler`) on `dragonmantank/cron-expression`: `job()`, `call()`,
  `cron()`, `everyMinute()` to `monthlyOn()`, `dailyAt()`, `timezone()`, `withoutOverlapping()`. Each due
  minute is claimed through a `SchedulerLock` (`MemorySchedulerLock`, `RedisSchedulerLock`,
  `DatabaseSchedulerLock`), so a task runs once across workers and servers.
- `App::background()` (`Nimbus\Runtime\Background`): the scheduler and queue workers run on the ticks PAM
  sends to task workers (`--task-workers N`), without a system cron; `Background::run()` is the loop for a
  dedicated process.
- `ArcanoMigrationManager::publish()` copies a package migration into the application's migrations.

## 0.2.0 - 2026-11-19

### Added
- `Input`: typed requests validated from public properties and attributes (`Min`, `Max`, `Email`,
  `Url`, `Uuid`, `Regex`, `In`, `Field`), nested inputs, enums and dates; controllers and closures
  receive them already validated, and `Input::schema()` feeds the OpenAPI document.
- `Paginator::paginate()` (numbered pages) and `Paginator::cursor()` (keyset); `JsonResource::collection()`
  adds their `meta` and `links`.
- `auth()` and `can()` on routes and route groups.
- `docs()` serves `openapi.json` and an API reference page.
- Redis stores for rate limits, idempotency and response caching over a per-command pool.
- `AsyncPostgresDriver` (opt-in, `DB_ASYNC=true`): with ext-pgsql, Postgres queries suspend the
  request Fiber instead of the worker; an interrupted command closes the session instead of returning
  it to the pool. Blocking PDO stays the default, being faster for short queries. Requires
  `pushinbr/arcano` 0.2.3.
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
