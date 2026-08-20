# PAM API

Express-like routing. Laravel-like application structure. PAM-native execution.

**[Official documentation](https://push-in.github.io/pam-docs/packages/api/) ·
[PAM introduction](https://push-in.github.io/pam-docs/introduction/) ·
[Report an issue](https://github.com/push-in/pam-api/issues)**

```bash
pam composer require pushinbr/pam-api
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

## Container lifetimes

```php
$app->container()->bind(UserRepository::class, DatabaseUserRepository::class);
$app->container()->singleton(Cache::class, RedisCache::class);
$app->container()->scoped(CurrentUser::class);
```

`scoped` values are created once per request and discarded even when the
handler throws. This boundary is essential for PAM's persistent workers.

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

## Quality gate

```bash
composer install
composer verify
```

The verification gate runs PHPStan at level 9 and the PHPUnit suite on every
supported PHP version.

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
    ->assertStatus(200)
    ->assertJsonPath('data.status', 1);
```

Run `composer benchmark` for the standalone router benchmark.

## License

Free and open-source under the [Apache License 2.0](LICENSE). You may use,
modify, and distribute this package for any purpose, including commercially.


## Recommended PAM workflow

Start new applications with `pam init my-api --template api`. In an existing PAM project, install the higher-level router with `pam composer require pushinbr/pam-api`; PAM runs Composer inside its private Embed SAPI.

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
- [Report an issue](https://github.com/push-in/pam-api/issues)

Report security vulnerabilities through GitHub private vulnerability reporting or the PAM security policy, not a public issue.
