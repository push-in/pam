# PAM HTTP

Express-like routing. Laravel-like application structure. PAM-native execution.

**[Official documentation](https://push-in.github.io/pam-docs/packages/http/) ·
[PAM introduction](https://push-in.github.io/pam-docs/introduction/) ·
[Upgrade guide](UPGRADE.md) · [Changelog](CHANGELOG.md) ·
[Report an issue](https://github.com/push-in/pam-http/issues)**

## Start here

PAM HTTP is a Composer package for the PAM runtime; it is not a standalone
server. [Install PAM](https://push-in.github.io/pam-docs/getting-started/installation/)
first, open your application directory, and let PAM run Composer for you:

```bash
pam composer require pushinbr/pam-http
```

```php
use Pam\App;
use Pam\Api\RouteConstraint;

$app = new App();

$app->post('/login', [LoginController::class, 'onLogin']);

$app->get('/users/{id}', [UserController::class, 'show'])
    ->where('id', RouteConstraint::Integer)
    ->name('users.show');

$app->listen(3000);
```

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

## Eloquent ORM (default)

PAM API ships with Laravel's Eloquent as its official ORM. The integration
keeps a separate connection manager per request Fiber, so connections,
transactions and mutable database state cannot leak between persistent-worker
requests.

```php
use Pam\Api\Database\DatabaseConfig;
use Pam\Api\Database\EloquentServiceProvider;

$app->provider(new EloquentServiceProvider(DatabaseConfig::fromEnvironment()));
```

Configure `DB_CONNECTION` (`pgsql`, `mysql`, `sqlite` or `sqlsrv`) and the
usual `DB_HOST`, `DB_PORT`, `DB_DATABASE`, `DB_USERNAME` and `DB_PASSWORD`
variables. PostgreSQL is the production default; SQLite is convenient for
tests. Models are ordinary `Illuminate\Database\Eloquent\Model` classes.

```php
final class User extends \Illuminate\Database\Eloquent\Model
{
    protected $fillable = ['name', 'email'];
}
```

Resolve `EloquentManager` when a service needs an explicit connection, schema
builder or transaction. Request cleanup automatically rolls back unfinished
transactions and disconnects every configured connection. Register
`DatabaseHealthCheck` in a `HealthRegistry` for readiness checks; its result
reports latency and failure class without exposing credentials.

Laravel-compatible anonymous-class migrations work through
`MigrationManager::migrate()` and `rollback()`. The manager creates the
migration repository on first use and supports dry-run execution, named
connections and bounded step rollbacks.

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

Tenant-owned models can be protected once during application boot:

```php
$tenancy = $app->container()->get(TenantModelGuard::class);
$tenancy->protect(Order::class); // defaults to tenant_id
```

The guard adds an Eloquent global scope and assigns the tenant key on create.
It fails closed when no request-scoped `TenantContext` exists and rejects a
model explicitly assigned to another tenant. Use an unguarded administrative
model only for deliberate cross-tenant operations.

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

See the [PAM API 2 design and delivery contract](docs/API-2.md) for the complete
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

## Production building blocks

PAM API exposes small, replaceable contracts instead of choosing application
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

Start new applications with `pam init my-api --template api`. In an existing PAM project, install the higher-level router with `pam composer require pushinbr/pam-http`; PAM runs Composer inside its private Embed SAPI.

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
- [Report an issue](https://github.com/push-in/pam-http/issues)

Report security vulnerabilities through GitHub private vulnerability reporting or the PAM security policy, not a public issue.
