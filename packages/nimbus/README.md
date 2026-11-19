# Nimbus

The API-first PHP framework for the PAM runtime: a Laravel-like application
structure, one routes file, the Arcano ORM, and persistent PAM workers.

**[Official documentation](https://github.com/push-in/nimbus#readme) ·
[PAM introduction](https://push-in.github.io/pam-docs/introduction/) ·
[Upgrade guide](UPGRADE.md) · [Changelog](CHANGELOG.md) ·
[Architecture](docs/ARCHITECTURE.md) ·
[Report an issue](https://github.com/push-in/pam/issues)**

## Start here

Nimbus is a Composer package for the PAM runtime; it is not a standalone
server. [Install PAM](https://push-in.github.io/pam-docs/getting-started/installation/)
first, open your application directory, and let PAM run Composer for you:

```bash
pam composer require pushinbr/nimbus
```

Routes live in one file, `routes.php`, which receives the router as `$router`:

```php
use App\Controllers\ProductController;
use Nimbus\RouteRegistrar;

$router->get('ping', fn ($request, $response) => $response->json(['pong' => true]));

$router->controller(ProductController::class, function (RouteRegistrar $router): void {
    $router->get('products', 'index');
    $router->get('products/{product}', 'show');
    $router->post('products', 'store');
});

// index, store, show, update and destroy, plus extras bound to the same controller
$router->resource('users', UserController::class, function (RouteRegistrar $users): void {
    $users->post('{user}/ban', 'ban');
});
```

```php
// index.php
$app = new Nimbus\App();
$app->routes(__DIR__ . '/routes.php');
$app->listen(3000);
```

Standalone routes keep `[Controller::class, 'method']`; `where()`, `name()` and
`middleware()` chain on any route.

Controllers are resolved through the container. Both constructor dependencies
and action parameters are injected:

```php
final readonly class LoginController
{
    public function __construct(private LoginService $login) {}

    public function onLogin(LoginRequest $request): AuthResource
    {
        return new AuthResource($this->login->handle($request->validated()));
    }
}
```

## Arcano ORM (optional)

Arcano is the PAM data mapper, derived from Cycle ORM: attribute entities,
relations, an identity map, migrations and SQLite, MySQL, PostgreSQL and SQL
Server drivers. Nimbus keeps its kernel independent from any database; install
Arcano when the application needs one:

```bash
pam composer require pushinbr/arcano
```

Entities are plain classes with attributes:

```php
use Arcano\Annotated\Annotation\{Column, Entity};
use Arcano\Annotated\Annotation\Relation\HasMany;

#[Entity(role: 'author')]
class Author
{
    #[Column(type: 'primary')]
    public ?int $id = null;

    #[Column(type: 'string(120)')]
    public string $name = '';

    /** @var list<Post> */
    #[HasMany(target: Post::class, collection: 'array')]
    public array $posts = [];
}
```

The provider compiles the entities of a directory and keeps the schema in a PHP
cache file. In development the cache is rebuilt when an entity file changes; in
production pass `refresh: false` and build the cache during deployment.

```php
use Nimbus\Database\{ArcanoSchemaCompiler, ArcanoServiceProvider, DatabaseConfig};

$app->provider(new ArcanoServiceProvider(
    DatabaseConfig::fromEnvironment(),
    new ArcanoSchemaCompiler(__DIR__ . '/src/Entities', __DIR__ . '/var/arcano-schema.php', refresh: $debug),
    migrations: __DIR__ . '/database/arcano',
));
```

A compiled schema array (or `Arcano\ORM\Schema`) is accepted in place of the
compiler. The same `DB_*` variables configure the connection.

Every request Fiber gets its own connections and identity map, so transactions
and loaded entities cannot leak between persistent-worker requests. Request
cleanup rolls back unfinished transactions and disconnects. Resolve
`ArcanoManager` to work with entities:

```php
$arcano = $app->container()->get(ArcanoManager::class);

$author = $arcano->select(Author::class)->load('posts')->wherePK($id)->fetchOne();
$author->name = 'Ada';
$arcano->entityManager()->persist($author)->run();
```

### Migrations

`ArcanoMigrationManager` writes migrations from the difference between the
entities and the database, runs them and rolls them back. Each migration runs
in its own transaction and is recorded in the `migrations` table.

```php
$migrations = $app->container()->get(ArcanoMigrationManager::class);

$migrations->generate();   // new files in database/arcano for the changed entities
$migrations->migrate();    // runs the pending ones, returns their names
$migrations->rollback(1);  // reverts the last one
$migrations->status();     // name, created_at, executed_at of every migration
```

Generated files can be edited, and migrations can be written by hand with the
`Arcano\Migrations\Migration` table API. Generation requires every existing
migration to have run, so the database reflects them.

### Transactions, budgets and tenants

`ArcanoManager` is the `TransactionManager` of the application, and
`DatabaseHealthCheck` and `QueryMonitor` budgets read its connections.
Tenant-owned roles are protected once during boot with
`$arcano->protectTenant(Order::class)`: selects are limited to the current
tenant, new entities get its key, and both fail closed without a `TenantContext`.

### Connections, health and query budgets

Configure `DB_CONNECTION` (`pgsql`, `mysql`, `sqlite` or `sqlsrv`) and the
usual `DB_HOST`, `DB_PORT`, `DB_DATABASE`, `DB_USERNAME` and `DB_PASSWORD`
variables, read by `DatabaseConfig::fromEnvironment()`. PostgreSQL is the
production default; SQLite is convenient for tests. Each request Fiber gets its
own connections; request cleanup rolls back unfinished transactions and
releases them. Register `DatabaseHealthCheck` in a `HealthRegistry` for
readiness checks; its result reports latency and failure class without exposing
credentials.

Use `QueryBudgetMiddleware` during development and in performance-sensitive
routes to cap query count, accumulated database time and repeated SQL patterns:

```php
$app->middleware(new QueryBudgetMiddleware(
    monitor: $app->container()->get(QueryMonitor::class),
    budget: new QueryBudget(
        maximumQueries: 30,
        maximumElapsedMilliseconds: 100,
        maximumDuplicateQueries: 3,
    ),
    failOnViolation: true,
));
```

Budgets are Fiber-local. Violations use the sequential integer-backed
`QueryBudgetViolation` enum, making them stable for CI reports and telemetry.

## Route groups

```php
$app->prefix('/api/v1')
    ->middleware(Authenticate::class)
    ->group(function (RouteRegistrar $routes): void {
        $routes->apiResource('/users', UserController::class);
        $routes->post('/login', [LoginController::class, 'onLogin']);
    });
```

Global, group and route middleware use the same PAM middleware contract.

`head()` and `options()` are available on both `App` and `RouteRegistrar`.
When no explicit `OPTIONS` route exists, PAM returns `204` with a deterministic
`Allow` header; every `GET` route automatically advertises and matches `HEAD`.
PAM suppresses the final `HEAD` body after middleware and error handling while
preserving its status and headers. An explicit `OPTIONS` or `HEAD` handler
always takes precedence.

Router configuration is bounded before the application freezes: 10,000 routes
by default (configurable up to 100,000), 2 KiB paths, 128 segments, 32 route
parameters and 512-byte custom constraints. Compiled patterns carry PCRE match
and depth budgets, and oversized untrusted request paths bypass PCRE entirely.

## Container lifetimes

```php
$app->container()->bind(UserRepository::class, DatabaseUserRepository::class);
$app->container()->singleton(Cache::class, RedisCache::class);
$app->container()->scoped(CurrentUser::class);
```

`scoped` values are created once per request and discarded even when the
handler throws. This boundary is essential for PAM's persistent workers.

## Typed configuration

Define configuration once and fail during boot when required environment is
missing or malformed:

```php
$config = Configuration::fromEnvironment([
    new ConfigDefinition('app.port', 'PAM_PORT', ConfigType::Integer),
    new ConfigDefinition('auth.secret', 'AUTH_SECRET', sensitive: true),
]);

$port = $config->integer('app.port');
```

Supported types are represented by the sequential integer-backed `ConfigType`
enum. Diagnostics should use `$config->redacted()`, which deterministically
masks sensitive values. Validation errors name the variable and expected type
without including its supplied value.

## Validation and resources

```php
final class LoginRequest extends FormRequest
{
    public function rules(): array
    {
        return [
            'email' => ['required', 'string', 'email'],
            'type' => ['required', Rule::enum(UserType::class)],
        ];
    }
}

enum UserType: int
{
    case Regular = 1;
    case Administrator = 2;
}
```

Return a `JsonResource` from a handler to receive a consistent `data` envelope.
Validation failures use Problem Details with stable sequential integer codes.

### Problem Details

Routing failures, `HttpException` instances and unexpected exceptions use one
safe Problem Details envelope with `application/problem+json`:

```json
{"type":"https://pam.dev/problems/5","title":"Version conflict.","status":409,"code":5}
```

Exception details may add domain fields but cannot replace `type`, `title`,
`status` or `code`. Unexpected exception messages are logged and never returned
to the client. `405` responses retain their deterministic `Allow` header and
use the sequential `MethodNotAllowed=9` problem code.

## Quality gate

```bash
composer install
composer verify
```

The verification gate runs PHPStan at level 9 and the PHPUnit suite on every
supported PHP version. It also compares all public classes, interfaces, enums,
methods, parameters, properties and constants against `api-surface.json`:

```bash
composer api:compat
```

The gate rejects unreviewed additions as baseline drift and rejects removed
symbols/members, signature changes, new interface methods, newly abstract/final
contracts and enum changes as incompatibilities. Run `bin/api-compat update`
after approving a compatible public addition. A breaking baseline update is
reserved for an intentional major version whose migration guide and changelog
are ready.

The same reviewed baseline generates the complete
[public API reference](docs/PUBLIC-API.md). `composer verify` rejects stale
reference output; regenerate it only after explicitly accepting compatible API
additions:

```bash
composer docs:generate
```

See the [Nimbus design and delivery contract](docs/API-2.md) for the complete
15-track implementation plan and current delivery status.

## Distributed rate limiting

`RateLimitMiddleware` uses a bounded in-memory token bucket by default and
accepts any `RateLimitStore` for process-wide or distributed enforcement:

```php
$app->middleware(new RateLimitMiddleware(
    requestsPerSecond: 20,
    burst: 40,
    store: $redisRateLimitStore,
    keyResolver: static fn (Request $request): string =>
        'token:' . $request->getHeader('authorization', 'anonymous'),
));
```

The middleware emits limit/remaining/retry headers and a Problem Details `429`
response. Applications behind proxies must supply a key resolver that trusts
only their explicitly configured proxy boundary.

## Signed bearer tokens

`HmacTokenCodec` provides a strict HS256 access-token foundation with bounded
token/payload sizes, constant-time signature and issuer/audience checks,
`iat`/`nbf`/`exp` validation, unique token IDs and abilities:

```php
$tokens = new HmacTokenCodec(
    secret: $secretFromYourSecretManager,
    issuer: 'https://auth.example.com',
    audience: 'orders-api',
);
$app->middleware(new AuthenticateMiddleware(
    new BearerTokenAuthenticator($tokens),
    $app->container(),
));
```

Set `keyIdentifier` on the active signing key and pass at most four retiring
keys through `verificationKeys` for bounded zero-downtime rotation. Unknown
`kid` values fail closed before claims are trusted. `BearerTokenAuthenticator`
also accepts a `TokenRevocationStore`; `MemoryTokenRevocationStore` is bounded
and intended for development/tests, while clustered applications should back
the contract with an atomic shared store.

Signing secrets must contain at least 32 bytes and must not be stored in source
control. Access tokens are capped at 24 hours. Refresh-token rotation,
OAuth/OIDC authorization-server duties belong to the application
or a dedicated identity provider; they must not be simulated with long-lived
access tokens.

## File storage

`Nimbus\Storage` puts named disks over [league/flysystem](https://flysystem.thephpleague.com)
v3. Nimbus does not require it: install `league/flysystem` for local disks and
`league/flysystem-aws-s3-v3` for S3 (also AWS-compatible stores such as MinIO and R2).

```php
$app->provider(new StorageServiceProvider(
    StorageConfig::fromEnvironment(),
    routes: '/files', // opt-in signed upload and download routes for local disks
));

$disk = $app->storage()->disk();           // FILESYSTEM_DISK, `local` by default
$disk->put('avatars/42.png', $bytes);
$disk->get('avatars/42.png');
$disk->url('avatars/42.png');               // STORAGE_URL / AWS_URL prefix, or the S3 object URL
$disk->temporaryUrl('invoices/7.pdf', 300); // download URL valid for 5 minutes
```

`StorageConfig::fromEnvironment()` defines `local` from `STORAGE_ROOT` (default `./storage`)
and `STORAGE_URL`, and `s3` when `AWS_BUCKET` is set (`AWS_DEFAULT_REGION`, `AWS_ENDPOINT`,
`AWS_ACCESS_KEY_ID`, `AWS_SECRET_ACCESS_KEY`, `AWS_USE_PATH_STYLE_ENDPOINT`, `AWS_URL`).
`APP_URL` prefixes the signed routes. Pass an array of disks to the constructor for more,
or wrap any flysystem operator with `StorageManager::filesystem($name, $operator)`. A
`Disk` is injectable and resolves to the default disk; `Disk::filesystem()` exposes the
flysystem operator for listing, moving and visibility.

### Direct uploads

`temporaryUploadUrl()` returns a URL the client sends the file to with `PUT`, so the
upload does not pass through your handler:

```php
$app->post('/avatars', static fn (Disk $disk, Response $response): Response => $response->json([
    'upload' => $disk->temporaryUploadUrl('avatars/42.png', ttl: 600, maxBytes: 2_000_000, contentType: 'image/png'),
]));
```

On S3 the URL is a presigned `PutObject` from the AWS SDK; S3 binds the content type
but cannot cap the size of a presigned PUT, so check `Disk::size()` once the client
reports the upload. On local disks it points at `PUT {routes}/upload`, which answers
`413` above `maxBytes`, `415` for another content type and `403` for a tampered or
expired URL, then writes the body and answers `201` with `{disk, path, size}`. The
runtime buffers request bodies up to the `maxBodyBytes` listen option (2 MiB by
default); raise it for larger local uploads or use S3. `GET {routes}/download` streams
the file behind a local `temporaryUrl()`.

### Signed URLs

`UrlSigner` signs a path and its query with HMAC-SHA256 under a key derived from
`APP_KEY`; changing the path, any parameter or the expiry invalidates the signature,
which is compared in constant time. `signed()` guards a route or a group and answers
`403` with problem code `InvalidSignature`:

```php
$signer = $app->container()->get(UrlSigner::class);
$link = $signer->temporary('/reports/7', ttl: 3600, query: ['format' => 'csv']);

$app->get('/reports/{id}', ReportController::class)->signed();
```

## Production building blocks

Nimbus exposes small, replaceable contracts instead of choosing application
infrastructure:

- authenticators, principals and ability checks;
- idempotency and response-cache stores;
- request-scoped tenant resolution;
- transactions, events and bounded jobs;
- retry, circuit breakers and cooperative deadlines;
- normalized observations, health checks and scope diagnostics.

Shared production state belongs in atomic Redis/database/broker adapters. The
included memory stores are bounded and intended for development and tests.

`JobQueue` defines reservation leases, acknowledgement, delayed release and
dead-letter transitions for durable adapters. The bounded memory implementation
and `JobWorker` exercise the same at-least-once lifecycle in tests:

```php
$queue = new MemoryJobDispatcher();
$queue->dispatch(new SendInvoice($invoiceId), maximumAttempts: 3);

$worker = new JobWorker(
    $queue,
    static fn (object $job): JobOutcome => JobOutcome::Complete,
);
$worker->runOne();
```

Attempts increment when a lease is reserved. Expired leases can be reclaimed;
exceptions and explicit retry outcomes release the job with a bounded delay,
and exhausted jobs enter the dead-letter set. Only failure class names are
retained, never exception messages that may contain application data.

Cross-process adapters should encode only `SerializableJob` implementations
through `JobCodec`. The codec uses an explicit name-to-class allowlist, a
versioned JSON envelope, a 64 KiB size limit and a depth limit. It never accepts
PHP serialized objects or client-supplied class names.

## Queues and the scheduler

Two durable `JobQueue` drivers share the memory queue's lifecycle and add a
failed-jobs store (`FailedJobRepository`: `failed()`, `retryFailed()`,
`forgetFailed()`). Both accept only `SerializableJob` jobs, encoded by `JobCodec`.

```php
$codec = new JobCodec([SendInvoice::class, SendReport::class]);

$queue = new RedisJobQueue(RedisPool::fromEnvironment(), $codec, queue: 'default');
// or, with the Arcano integration:
$queue = new DatabaseJobQueue($databases, $codec, queue: 'default');

$queue->dispatch(new SendInvoice($invoiceId), maximumAttempts: 5, delaySeconds: 60);

$worker = new JobWorker(
    $queue,
    new InvokableJobHandler($app->container()), // calls $job->handle(...) with injection
    leaseSeconds: 60,
    retryPolicy: new ExponentialBackoff(baseSeconds: 5, maximumSeconds: 3_600),
);
```

- **Redis**: pending jobs are a sorted set scored by availability (delays and
  retries need nothing else); a reservation moves the job to a lease set. Every
  transition is one Lua script, and keys share a `{queue}` hash tag for Redis
  Cluster.
- **Database**: tables `jobs`, `failed_jobs` and `scheduler_locks`. Publish the
  migration once, then migrate:
  `$migrations->publish('create_queue_tables', QueueTablesMigration::class)`.
  Postgres and MySQL reserve with `SELECT ... FOR UPDATE SKIP LOCKED`; SQLite
  (and SQL Server) reserve with a compare-and-swap update.
- A worker that dies mid-job leaves its lease to expire and the job returns to
  the queue; a stale worker cannot complete or release a job someone else took
  over, because every transition checks the attempt count of its reservation.
  A job whose last attempt expired, a dead-lettered job and a payload that no
  longer decodes all land in the failed jobs, with the exception class only.
- `ExponentialBackoff` doubles the delay per attempt up to a ceiling, with
  equal jitter (between half and all of the delay) by default.

The scheduler needs `dragonmantank/cron-expression`
(`composer require dragonmantank/cron-expression`):

```php
$schedule = new Schedule($queue, timezone: 'America/Sao_Paulo', container: $app->container());
$schedule->job(SendReport::class)->cron('0 * * * *');
$schedule->job(new PruneSessions())->dailyAt('03:00');
$schedule->call(fn () => $cache->prune())->everyFiveMinutes()->withoutOverlapping()->name('prune-cache');

$app->background()
    ->scheduler(new Scheduler($schedule, new RedisSchedulerLock(RedisPool::fromEnvironment())))
    ->queue($worker);
```

Frequencies: `cron()`, `everyMinute()`, `everyTwoMinutes()`, `everyFiveMinutes()`,
`everyTenMinutes()`, `everyFifteenMinutes()`, `everyThirtyMinutes()`, `hourly()`,
`hourlyAt()`, `daily()`, `dailyAt()`, `weekly()`, `weeklyOn()`, `monthly()`,
`monthlyOn()`, plus `timezone()` per task. Each due task first claims its
(task, minute) key in the `SchedulerLock`, so it runs once per minute however
many workers or servers tick: use `RedisSchedulerLock` or `DatabaseSchedulerLock`
whenever more than one process runs the scheduler; `MemorySchedulerLock` (the
default) only covers one. `withoutOverlapping()` also skips a run while the
previous one holds its lock. Task names identify them across processes; jobs are
named after `jobName()` and callbacks after their file and line, so name
callbacks with `name()` when that is not stable.

### Running inside PAM

No system cron and no extra process: start the server with task workers and
the scheduler and queue run on them.

```bash
pam start index.php --workers 4 --task-workers 1
```

The PAM master sends one tick per second to the task-worker pool, authenticated
by the cluster's token and never two at once. `App::handle()` answers it by
calling `Background::tick()`, which runs the scheduler and then drains the queue
for at most 100 jobs or 0.8 s, so the next tick is not held up. HTTP workers never
run background work, and the tick header from public traffic is ignored.

Without task workers (a plain `php` process, a separate container), run the same
`Background` as a loop; it sleeps through `Pam\Async\delay()` under PAM and stops
on SIGTERM or SIGINT:

```php
$background->run(intervalSeconds: 1.0);
```

## Request lifecycle observers

Register a `RequestLifecycleObserver` with `$app->observe()` to build profilers,
traces and leak diagnostics around the complete request pipeline. Observers
start in registration order and finish in reverse order, receive the handled
failure when one occurred, and execute before the request scope is destroyed.
An observer cleanup failure is logged and cannot prevent later observers or
container cleanup from running.

`MemoryProfiler` is the bounded first PAM Lens collector. Enable it explicitly
with `ProfilerMode::Development` or `ProfilerMode::Testing` and register it as
an observer. It emits `X-Debug-Token` and stores only method, path, status,
duration, memory delta and failure class; request/response bodies and headers
are never retained. Disabled mode is the default and emits nothing.

## OpenAPI and generated clients

```php
$app->post('/users', [UserController::class, 'store'])
    ->name('users.store')
    ->summary('Create a user')
    ->tags(['Users'])
    ->input(StoreUserRequest::class)
    ->output(UserResource::class);

$contract = $app->openApi('My API', '1.0.0');
$openapi = $contract->toJson();
$typescript = $contract->client(ClientLanguage::TypeScript);
$kotlin = $contract->client(ClientLanguage::Kotlin);
$swift = $contract->client(ClientLanguage::Swift);
```

`CompatibilityChecker` reports breaking path and operation removals using
sequential integer codes.

## In-memory testing

```php
(new TestClient($app))
    ->postJson('/login', ['email' => 'dev@pam.dev'])
    ->assertSuccessful()
    ->assertHeader('content-type', 'application/json')
    ->assertJsonPath('data.status', 1);
```

Use `assertJson()` for an exact payload, `assertJsonPath()` for a focused value,
and `assertStatus()` when the endpoint intentionally returns a specific code.

Run `composer benchmark` for the standalone router benchmark. Its schema 1 JSON
reports warmed static and constrained-dynamic throughput, p50/p95/p99 latency,
peak memory and the exact regression budget. `composer verify` runs
`benchmark:check` with intentionally conservative ceilings to catch severe
regressions while avoiding hardware-specific marketing claims.

## License

Free and open-source under the [Apache License 2.0](LICENSE). You may use,
modify, and distribute this package for any purpose, including commercially.

See [CONTRIBUTING.md](CONTRIBUTING.md) to contribute and [SECURITY.md](SECURITY.md)
for private vulnerability reporting.


## Recommended PAM workflow

Start new applications with `pam init my-api --template api`. In an existing PAM project, install the higher-level router with `pam composer require pushinbr/nimbus`; PAM runs Composer inside its private Embed SAPI.

Run `pam doctor` after dependency changes and before creating a release. The project remains a normal Composer project with a standard manifest, lockfile, PSR-4 autoloading, and `vendor/autoload.php`.

## API guide

| Surface | Use it for |
| --- | --- |
| `App` | Register routes, middleware, providers, error boundaries, and the listener. |
| `Router` | Compile and match method/path routes with typed results. |
| `Pipeline` | Execute middleware and the destination handler in order. |
| `CorsMiddleware` | Apply explicit origin, method, and header policy. |
| `RateLimitMiddleware` | Apply bounded per-key request limits. |
| `SecurityHeadersMiddleware` | Set conservative browser security headers. |

Route parameters are available through `$request->route()`. A path that exists for another method produces 405 behavior; an unknown path produces 404 behavior. Register error handling with `onError()` and keep transport-level timeouts and request limits in the PAM listener options.

## Production checklist

- Keep request data and mutable state scoped to the current request.
- Test success, validation failure, exception, cancellation, and timeout paths.
- Configure explicit limits and avoid unbounded payloads, queues, or retained collections.
- Run `pam doctor`, `pam test`, and the relevant integration suite before release.
- Validate real dependencies and workload behavior; compatibility is not inferred from package installation alone.

## Troubleshooting

- **Class not found:** run `pam composer install`, verify PSR-4 configuration, and rerun `pam doctor`.
- **Behavior differs over the network:** reproduce with PAM's transport integration tests; in-memory execution does not model the socket boundary.
- **A dependency blocks a worker:** use PAM-native I/O, a compatible event loop, a process pool, or additional isolated workers.

## Documentation and support

- [PAM introduction](https://push-in.github.io/pam-docs/introduction/)
- [Package ecosystem](https://push-in.github.io/pam-docs/packages/overview/)
- [Runtime compatibility](https://push-in.github.io/pam-docs/runtime/compatibility/)
- [Report an issue](https://github.com/push-in/pam/issues)

Report security vulnerabilities through GitHub private vulnerability reporting or the PAM security policy, not a public issue.
