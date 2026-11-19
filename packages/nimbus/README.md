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
control. Access tokens are capped at 24 hours; renew them with the rotating
refresh tokens below rather than issuing long-lived access tokens.

## API authentication

Publish the tables once (`personal_access_tokens`, `refresh_tokens`,
`auth_tokens`, `revoked_tokens`, `oauth_states`):

```php
$migrations->publish('create_auth_tables', \Nimbus\Auth\AuthTablesMigration::class);
```

Every stored token is the SHA-256 of a 256-bit random secret, compared in
constant time; plain tokens exist only in the response that hands them out.

### Personal access tokens

```php
$tokens = new PersonalAccessTokens($databases);           // ArcanoDatabaseProvider
$new = $tokens->create('user:42', 'CLI', ['products:read', 'products:write']);
return $response->json($new->toArray(), 201);              // shows the plain token once

$app->container()->instance(Authenticator::class, new PersonalAccessTokenAuthenticator($tokens));
$app->post('/products', CreateProduct::class)->can('products:write');
```

Tokens read `nbp_{id}.{secret}`, may expire (`expiresAt`), record
`last_used_at` (at most once a minute) and are revoked with `revoke($id)` or
`revokeAll($subject)`. Abilities accept `*` and namespace wildcards
(`products:*`, `orders.*`). `ChainAuthenticator` combines authenticators, e.g.
JWT access tokens first and personal tokens next.

### Access and refresh tokens

```php
$pairs = new TokenPairIssuer($codec, new RefreshTokens($databases), accessTtlSeconds: 900);
$pair = $pairs->issue('user:42', ['orders:read']);   // after checking the password with Hash::check()
return $response->json($pair->toArray());            // token_type, access_token, expires_in, refresh_token, scope
```

`$codec` is an `HmacTokenCodec` or a `JwtCodec`. `refresh()` spends the refresh
token and returns a new pair of the same family; a spent token presented again
revokes the whole family (reuse detection) and answers 401.
`RefreshTokens::revokeAll($subject)` logs a user out everywhere. Revoke a
stateless access token by its `jti` (`TokenPrincipal::$tokenIdentifier`) in a
shared `TokenRevocationStore`: `DatabaseTokenRevocationStore` or
`Nimbus\Redis\RedisTokenRevocationStore` (the memory store is per worker).

### JWT

`JwtCodec` wraps `firebase/php-jwt` (`composer require firebase/php-jwt`):

```php
$jwt = JwtCodec::hmac($secret, 'https://api.example.com', 'orders-api');               // HS256/384/512
$jwt = JwtCodec::asymmetric($publicPem, $privatePem, $issuer, $audience, 'ES256');     // RS256, ES256, EdDSA
$jwt = JwtCodec::fromJwks($jwksDocument, 'https://idp.example.com', 'orders-api');     // verify only
$app->container()->instance(Authenticator::class, new JwtAuthenticator($jwt, $revocations));
```

Keys are bound to their algorithm and tokens must carry `kid`, `iss`, `aud`,
`sub` and `exp`. Abilities travel in the `scope` claim (`scp` and `abilities`
arrays are accepted). `JwtCodec::fromKeySet(new CachedKeySet(...))` verifies
against a remote JWKS cached through PSR-18/PSR-6.

### Social login

`SocialLogin` runs the authorization-code flow of any `league/oauth2-client`
provider for API clients: `GET /auth/oauth/{provider}` returns
`authorization_url` and `state`, the state and PKCE verifier stay on the
server, and `POST /auth/oauth/{provider}/callback` (`code`, `state`) exchanges
the code and returns a Nimbus token.

```php
$social = new SocialLogin(
    ['google' => new Google([...]), 'github' => fn () => new Github([...])],
    new DatabaseOAuthStateStore($databases),   // or Nimbus\Redis\RedisOAuthStateStore
    $app->container()->get(SocialUsers::class), // your SocialUserResolver: find or create the user
    $pairs,                                     // any LoginTokenIssuer
    abilities: ['*'],
);
```

### Password reset and e-mail verification

Implement `AuthUser` on the user and `AuthUserProvider` on its repository, then
register the routes:

```php
$oneTime = new OneTimeTokens($databases);
(new AuthRoutes(
    passwords: new PasswordResets($users, $oneTime, $notifier, 'https://app.example.com/reset?token={token}',
        afterReset: fn (AuthUser $user) => $refreshTokens->revokeAll($user->authIdentifier())),
    verification: new EmailVerification($users, $oneTime, $notifier, 'https://app.example.com/verify?token={token}'),
    tokens: $pairs,
    social: $social,
    throttleStore: new RedisRateLimitStore($redis),
))->register($app, '/auth');
```

| Route | Body | Answer |
| --- | --- | --- |
| `POST /auth/password/forgot` | `email` | 202, the same for unknown addresses |
| `POST /auth/password/reset` | `token`, `password` | 204 or 422 |
| `POST /auth/email/verify` | `token` | 204 or 422 |
| `POST /auth/email/resend` | authenticated | 202 |
| `POST /auth/token/refresh` | `refresh_token` | token pair or 401 |
| `POST /auth/token/revoke` | `refresh_token` | 204 |

Reset and verification tokens are single-use, expire (one hour and one day by
default) and replace the user's previous token. Links go out through the
Notifications module with `send()`, never queued, so tokens stay out of job
payloads; pass `notification:` to customise the `MailMessage`. Each user gets
at most one e-mail a minute and every route is rate limited per client IP.
New passwords are stored with `Nimbus\Security\Hash`.

### Gates and policies

```php
$gate = $app->gate();
$gate->before(fn (Principal $user, string $ability) => $user->identifier() === 'user:1' ? true : null);
$gate->define('reports:export', fn (Principal $user) => $user->can('reports:*'));
$gate->policy(Product::class, ProductPolicy::class);   // update(Principal $user, Product $product): bool

$app->put('/products/{product}', UpdateProduct::class)->can('update', 'product');
$app->get('/products', ListProducts::class)->can('view-any', Product::class);
$app->post('/products', CreateProduct::class)->can('products:write');
```

`can($ability, 'product')` resolves the route parameter through `bindRoute()`
or `RouteBindable` and asks the policy method named after the ability
(`view-any` calls `viewAny`); policies are built by the container and may have
their own `before()`. Abilities without a policy or definition fall back to
the principal's `can()`, i.e. the token's abilities, so a policy can also
demand a scope (`$user->can('products:write')`). In handlers,
`$gate->authorize('update', $product)` throws 403 problem details
(401 without a principal); `allows()` and `denies()` return booleans.

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

## Web: sessions, CSRF, views and translation (optional)

API apps need none of this. For server-rendered pages, register the providers:

```php
use Nimbus\Http\CookieCodec;
use Nimbus\Redis\RedisSessionStore;
use Nimbus\Session\SessionServiceProvider;
use Nimbus\Translation\TranslationServiceProvider;
use Nimbus\View\ViewServiceProvider;

$app->provider(new SessionServiceProvider(new RedisSessionStore($redis), cookies: CookieCodec::fromEnvironment()));
$app->provider(new TranslationServiceProvider(__DIR__ . '/lang', 'pt_BR'));
$app->provider(new ViewServiceProvider(__DIR__ . '/resources/views'));
```

`SessionServiceProvider` adds, in order, method override (`_method`), encrypted cookies, the session
and CSRF checks. Sessions are stored as JSON in Redis, the database (`SessionTablesMigration`), files
or memory, and are only written when the request used them. Inject `Session` (flash data, old input,
errors, `regenerate()`), `CookieJar` and `RequestCookies`; `SessionAuthenticator::login()` regenerates
the session id. CSRF accepts `_token`, `X-CSRF-TOKEN` or `X-XSRF-TOKEN` and skips bearer requests.

Templates live in `resources/views/**/*.nim.php` and are compiled once per worker:

```blade
<x-layout title="Products">
    @foreach ($products as $product)
        <x-card :product="$product" class="mb-4">
            <x-slot:footer>{{ __('products.price', ['value' => $product->price]) }}</x-slot>
        </x-card>
    @endforeach
    <form method="POST" action="/products">@csrf
        <input name="name" value="{{ old('name') }}"> @error('name') {{ $message }} @enderror
    </form>
</x-layout>
```

```php
// resources/views/components/card.nim.php
@props(['product'])
<div {{ $attributes->merge(['class' => 'card']) }}>{{ $product->name }} {{ $footer ?? '' }}</div>
```

Return `view('products.index', ['products' => $products])` (or `$views->make()` from an injected
`ViewFactory`) from a route. Class components extend `Nimbus\View\Component` in `app/Views/`.
`vendor/bin/nimbus-views check` compiles every template and fails with `file:line: message`.
Translations come from `lang/{locale}/*.php` and `lang/{locale}.json` (`__('messages.welcome', ['name'
=> $name])`, `trans_choice()`); the locale is per request (`?lang=`, session, `Accept-Language`).

### Admin panel over Arcano (optional)

`Nimbus\Admin` serves CRUD pages for Arcano entities, with the fields read from their `#[Column]`
attributes and belongs-to relations:

```php
use Nimbus\Admin\Admin;

Admin::panel($app, '/admin', loginUrl: '/login')
    ->resource(ProductAdmin::class)   // an AdminResource subclass
    ->resource(Category::class);      // or just the entity, with the defaults

$app->gate()->define('viewAdmin', static fn (User $user): bool => $user->isAdmin);
```

Lists have search, filters (booleans, selects, belongs-to), sorting, pagination and bulk actions;
forms validate through the field rules (column size, `#[Max]`/`#[Min]`/... on the entity) or an
`Input` class. Every route needs `viewAdmin`; with a Gate policy the entity also needs `viewAny`,
`create`, `update` and `delete`. It requires the session (CSRF) and an `Authenticator`; templates are
`.nim.php` with a solid CSS (light and dark) and an optional 2 KB script. `pam make:admin Product`
creates `app/Admin/ProductAdmin.php`.

## Notifications

`Nimbus\Notifications` sends one `Notification` through the channels its
`via()` returns: `mail`, `push`, `database` and `webhook`, or channels of your
own (`NotificationChannel`). Recipients implement `Notifiable`; the
`RoutesNotifications` trait reads `routeNotificationFor<Channel>()` methods, an
`email` property for mail and an `id` property for the stored key.

```php
final class InvoicePaid extends Notification
{
    public function __construct(private readonly Invoice $invoice) {}

    public function via(Notifiable $notifiable): array
    {
        return ['mail', 'push', 'database', 'webhook'];
    }

    public function toMail(Notifiable $notifiable): MailMessage
    {
        return MailMessage::make()
            ->subject("Invoice {$this->invoice->number} paid")
            ->greeting('Hello')
            ->line('We received your payment.')
            ->action('View invoice', $this->invoice->url);
    }

    public function toPush(Notifiable $notifiable): PushMessage
    {
        return PushMessage::make('Invoice paid', $this->invoice->number)
            ->data(['invoice' => $this->invoice->id])
            ->collapseKey('invoices');
    }

    public function toDatabase(Notifiable $notifiable): array
    {
        return ['invoice' => $this->invoice->id, 'amount_in_cents' => $this->invoice->amountInCents];
    }

    public function toWebhook(Notifiable $notifiable): WebhookMessage
    {
        return WebhookMessage::make(['invoice' => $this->invoice->id])->event('invoice.paid');
    }
}

$sender = new NotificationSender([
    'mail' => MailChannel::fromEnvironment(),
    'push' => new PushChannel(
        FcmProvider::fromServiceAccountFile('/run/secrets/firebase.json'),
        ApnsProvider::fromKeyFile('TEAMID', 'KEYID', '/run/secrets/apns.p8', 'com.example.app'),
        onInvalidToken: static fn (PushTarget $target) => $devices->forget($target->token),
    ),
    'database' => new DatabaseChannel($databases),
    'webhook' => new WebhookChannel((string) getenv('WEBHOOK_SECRET')),
]);
$notifier = new Notifier($sender, $queue);

$notifier->send($user, new InvoicePaid($invoice));       // now
$notifier->sendLater($users, new InvoicePaid($invoice)); // one job per user and channel
```

Register `SendNotificationJob` in your `JobCodec` and run `SendNotificationHandler`
in a `JobWorker`. Channels render the notification when it is queued, so the job
payload is plain JSON and the worker never reloads the notifiable. Retryable
failures (timeouts, HTTP 408/429/5xx, SMTP errors) return `JobOutcome::Retry`;
permanent ones (other 4xx, invalid addresses) are dead-lettered. Override
`maximumAttempts()` and `delaySeconds()` on the notification to tune them.

- **Mail** uses `symfony/mailer` (`composer require symfony/mailer`).
  `MailChannel::fromEnvironment()` reads `MAILER_DSN`, or `MAIL_MAILER` as a DSN
  or `smtp` with `MAIL_HOST`/`MAIL_PORT`/`MAIL_USERNAME`/`MAIL_PASSWORD`, and
  `MAIL_FROM_ADDRESS`/`MAIL_FROM_NAME`. The route is an address, a list, or an
  `address => name` map. `html()` and `text()` replace the generated bodies.
- **Push** routes are `PushTarget` objects or `fcm:<token>` / `apns:<token>`
  strings. FCM uses HTTP v1 with the service account (RS256 assertion, OAuth2
  token cached per worker until five minutes before expiry). APNs uses token
  authentication (ES256 JWT reused for 50 minutes) over HTTP/2. Unregistered
  tokens are reported to `onInvalidToken` instead of failing the delivery.
- **Database** writes to the `notifications` table of
  `database/migrations/20261119.000000_0_create_notifications.php` (copy it into
  your migrations directory). `DatabaseNotifications` lists, counts, marks read
  or unread, deletes and prunes, always scoped to the notifiable.
- **Webhook** POSTs `{"id","event","created_at","data"}` with
  `Nimbus-Event`, `Nimbus-Delivery` (the notification id, for receiver
  deduplication) and `Nimbus-Signature: t=<unix>,v1=<hex HMAC-SHA256 of "t.body">`.
  Receivers check it with `WebhookSignature::verify($header, $body, $secret)`.
  The secret never enters the job payload; pass `secretFor:` for per-URL secrets.

Push and webhooks share the `HttpClient` interface. `HttpClients::detect()`
uses the non-blocking PAM client inside the runtime and ext-curl otherwise (and
for HTTP/2, which the PAM client does not speak); wrap any PSR-18 client with
`Psr18HttpClient`.

In tests, bind `Notifier::fake()` where a `NotificationDispatcher` is expected:

```php
$notifications = Notifier::fake();
// ... exercise the code ...
$notifications->assertSentTo($user, InvoicePaid::class);
$notifications->assertSentLaterTo($user, InvoicePaid::class);
$notifications->assertNothingSent();
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

`Nimbus\Testing\TestCase` (PHPUnit) builds and boots a fresh application per test and sends
requests to it in memory:

```php
use Nimbus\Testing\RefreshDatabase;
use Nimbus\Testing\TestCase;

final class ProductTest extends TestCase
{
    use RefreshDatabase;

    protected function createApplication(): App
    {
        $app = new App(discoverPackages: false);
        $app->provider(new ArcanoServiceProvider($config, $entities, migrations: $migrations));
        $app->routes(dirname(__DIR__) . '/routes.php');
        return $app;
    }

    public function testListsProducts(): void
    {
        ProductFactory::new()->count(3)->sequence(['name' => 'Keyboard'], ['name' => 'Mouse'])->create();

        $this->getJson('/products')
            ->assertOk()
            ->assertJsonCount(3, 'data')
            ->assertJsonPath('data.0.name', 'Keyboard')
            ->assertJsonStructure(['data' => ['*' => ['id', 'name']]]);

        $this->actingAs($user)
            ->postJson('/products', ['name' => ''])
            ->assertUnprocessableEntity()
            ->assertJsonValidationErrors(['name' => 'required']);

        $this->assertDatabaseCount('products', 3);
        $this->assertDatabaseHas(Product::class, ['name' => 'Mouse']);
    }
}
```

- Requests: `getJson`, `postJson`, `putJson`, `patchJson`, `deleteJson`, `json($method, ...)` and
  `get`; `withHeaders()`, `withHeader()` and `withToken()` apply to the following requests.
- `actingAs($principal)` binds an `Authenticator` returning the principal, so routes with
  `auth()`/`can()` and `AuthenticateMiddleware` resolved through the container see it.
- Responses: `assertOk`, `assertCreated`, `assertNoContent`, `assertStatus`, `assertNotFound`,
  `assertUnauthorized`, `assertForbidden`, `assertUnprocessableEntity`, `assertHeader`,
  `assertHeaderMissing`, `assertJson` (subset), `assertExactJson`, `assertJsonPath` (value or
  closure), `assertJsonMissingPath`, `assertJsonCount`, `assertJsonStructure` (`*` for lists),
  `assertJsonValidationErrors` and `assertJsonMissingValidationErrors` (the 422 Problem Details
  `errors` member), `json($path)` and `dump()`. Failures are PHPUnit assertion failures.
- `RefreshDatabase` runs the pending migrations, then opens a transaction that every request of the
  test shares and that is rolled back when the test ends. A request that leaves its own transaction
  open loses only its writes, as in production.
- `assertDatabaseHas`, `assertDatabaseMissing` and `assertDatabaseCount` take a table or an entity
  class.

Without the base class, `TestClient` offers the same requests over any `App`:

```php
(new TestClient($app))
    ->withToken($token)
    ->postJson('/login', ['email' => 'dev@pam.dev'])
    ->assertSuccessful()
    ->assertJsonPath('data.status', 1);
```

### Factories and seeders

Factories build Arcano entities with [Faker](https://fakerphp.org) (`composer require --dev
fakerphp/faker`). Attributes named like constructor parameters go to the constructor; the rest are
set as properties.

```php
/** @extends Factory<Product> */
final class ProductFactory extends Factory
{
    protected string $model = Product::class;

    public function definition(): array
    {
        return [
            'name' => $this->faker->words(2, true),
            'priceInCents' => $this->faker->numberBetween(100, 100_000),
            'category' => CategoryFactory::new(), // made, then persisted by cascade
        ];
    }

    public function archived(): self
    {
        return $this->state(['status' => ProductStatus::Archived]);
    }
}

ProductFactory::new()->makeOne();                      // unsaved
ProductFactory::new()->count(10)->archived()->create(); // persisted in one unit of work
ProductFactory::new()->state(fn (array $a) => ['name' => strtoupper($a['name'])])->raw();
```

`count()`, `state()`, `sequence()`, `afterMaking()` and `afterCreating()` return copies;
`make()`/`create()` return one entity, or a list after `count()` (`makeOne`, `makeMany`,
`createOne`, `createMany` are typed). `create()` persists through the manager set with
`Factory::useManager()`, which `TestCase` and `SeederRunner` set; `Factory::useFaker()` changes
the locale or seed.

Seeders define `run()`, resolved through the container, and chain others with `call()`:

```php
final class DatabaseSeeder extends Seeder
{
    public function run(ArcanoManager $arcano): void
    {
        $this->call([CategorySeeder::class, ProductSeeder::class]);
    }
}

(new SeederRunner($app))->run(DatabaseSeeder::class); // or $this->seed(...) in a TestCase
```

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
