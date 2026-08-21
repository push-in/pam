# PAM API public reference

> Generated from `api-surface.json` by `bin/api-reference`. Do not edit manually.

Every symbol below is protected by the PAM API Semantic Versioning gate.

## Symbols

- [`Pam\Api\Auth\AuthContext`](#pamapiauthauthcontext)
- [`Pam\Api\Auth\Authenticator`](#pamapiauthauthenticator)
- [`Pam\Api\Auth\BearerTokenAuthenticator`](#pamapiauthbearertokenauthenticator)
- [`Pam\Api\Auth\HmacTokenCodec`](#pamapiauthhmactokencodec)
- [`Pam\Api\Auth\MemoryTokenRevocationStore`](#pamapiauthmemorytokenrevocationstore)
- [`Pam\Api\Auth\Principal`](#pamapiauthprincipal)
- [`Pam\Api\Auth\TokenPrincipal`](#pamapiauthtokenprincipal)
- [`Pam\Api\Auth\TokenRevocationStore`](#pamapiauthtokenrevocationstore)
- [`Pam\Api\Cache\CacheRecord`](#pamapicachecacherecord)
- [`Pam\Api\Cache\MemoryResponseCacheStore`](#pamapicachememoryresponsecachestore)
- [`Pam\Api\Cache\ResponseCacheStore`](#pamapicacheresponsecachestore)
- [`Pam\Api\CallableRequestHandler`](#pamapicallablerequesthandler)
- [`Pam\Api\Config\ConfigDefinition`](#pamapiconfigconfigdefinition)
- [`Pam\Api\Config\ConfigType`](#pamapiconfigconfigtype)
- [`Pam\Api\Config\Configuration`](#pamapiconfigconfiguration)
- [`Pam\Api\Config\ConfigurationException`](#pamapiconfigconfigurationexception)
- [`Pam\Api\ContainerMiddleware`](#pamapicontainermiddleware)
- [`Pam\Api\Container\Binding`](#pamapicontainerbinding)
- [`Pam\Api\Container\BindingLifetime`](#pamapicontainerbindinglifetime)
- [`Pam\Api\Container\Container`](#pamapicontainercontainer)
- [`Pam\Api\Container\ContainerState`](#pamapicontainercontainerstate)
- [`Pam\Api\ControllerHandler`](#pamapicontrollerhandler)
- [`Pam\Api\Database\DatabaseConfig`](#pamapidatabasedatabaseconfig)
- [`Pam\Api\Database\DatabaseHealthCheck`](#pamapidatabasedatabasehealthcheck)
- [`Pam\Api\Database\EloquentManager`](#pamapidatabaseeloquentmanager)
- [`Pam\Api\Database\EloquentServiceProvider`](#pamapidatabaseeloquentserviceprovider)
- [`Pam\Api\Database\MigrationManager`](#pamapidatabasemigrationmanager)
- [`Pam\Api\Database\QueryBudget`](#pamapidatabasequerybudget)
- [`Pam\Api\Database\QueryBudgetExceededException`](#pamapidatabasequerybudgetexceededexception)
- [`Pam\Api\Database\QueryBudgetViolation`](#pamapidatabasequerybudgetviolation)
- [`Pam\Api\Database\QueryMonitor`](#pamapidatabasequerymonitor)
- [`Pam\Api\Database\QueryReport`](#pamapidatabasequeryreport)
- [`Pam\Api\Database\TenantModelGuard`](#pamapidatabasetenantmodelguard)
- [`Pam\Api\Database\TenantScope`](#pamapidatabasetenantscope)
- [`Pam\Api\Events\EventDispatcher`](#pamapieventseventdispatcher)
- [`Pam\Api\Events\SyncEventDispatcher`](#pamapieventssynceventdispatcher)
- [`Pam\Api\HandlerResolver`](#pamapihandlerresolver)
- [`Pam\Api\Health\HealthCheck`](#pamapihealthhealthcheck)
- [`Pam\Api\Health\HealthRegistry`](#pamapihealthhealthregistry)
- [`Pam\Api\Health\HealthResult`](#pamapihealthhealthresult)
- [`Pam\Api\Health\HealthState`](#pamapihealthhealthstate)
- [`Pam\Api\Http\ClientIpResolver`](#pamapihttpclientipresolver)
- [`Pam\Api\Http\HttpException`](#pamapihttphttpexception)
- [`Pam\Api\Http\JsonResource`](#pamapihttpjsonresource)
- [`Pam\Api\Http\ProblemCode`](#pamapihttpproblemcode)
- [`Pam\Api\Http\ResourceCollection`](#pamapihttpresourcecollection)
- [`Pam\Api\Http\Responsable`](#pamapihttpresponsable)
- [`Pam\Api\Http\ResponseSnapshot`](#pamapihttpresponsesnapshot)
- [`Pam\Api\Idempotency\IdempotencyRecord`](#pamapiidempotencyidempotencyrecord)
- [`Pam\Api\Idempotency\IdempotencyStore`](#pamapiidempotencyidempotencystore)
- [`Pam\Api\Idempotency\MemoryIdempotencyStore`](#pamapiidempotencymemoryidempotencystore)
- [`Pam\Api\Jobs\JobCodec`](#pamapijobsjobcodec)
- [`Pam\Api\Jobs\JobDispatcher`](#pamapijobsjobdispatcher)
- [`Pam\Api\Jobs\JobEnvelope`](#pamapijobsjobenvelope)
- [`Pam\Api\Jobs\JobHandler`](#pamapijobsjobhandler)
- [`Pam\Api\Jobs\JobOutcome`](#pamapijobsjoboutcome)
- [`Pam\Api\Jobs\JobQueue`](#pamapijobsjobqueue)
- [`Pam\Api\Jobs\JobRunResult`](#pamapijobsjobrunresult)
- [`Pam\Api\Jobs\JobState`](#pamapijobsjobstate)
- [`Pam\Api\Jobs\JobWorker`](#pamapijobsjobworker)
- [`Pam\Api\Jobs\MemoryJobDispatcher`](#pamapijobsmemoryjobdispatcher)
- [`Pam\Api\Jobs\SerializableJob`](#pamapijobsserializablejob)
- [`Pam\Api\Lifecycle\RequestLifecycleObserver`](#pamapilifecyclerequestlifecycleobserver)
- [`Pam\Api\Middleware\AuthenticateMiddleware`](#pamapimiddlewareauthenticatemiddleware)
- [`Pam\Api\Middleware\AuthorizeMiddleware`](#pamapimiddlewareauthorizemiddleware)
- [`Pam\Api\Middleware\CorsMiddleware`](#pamapimiddlewarecorsmiddleware)
- [`Pam\Api\Middleware\DeadlineMiddleware`](#pamapimiddlewaredeadlinemiddleware)
- [`Pam\Api\Middleware\EloquentLifecycleMiddleware`](#pamapimiddlewareeloquentlifecyclemiddleware)
- [`Pam\Api\Middleware\IdempotencyMiddleware`](#pamapimiddlewareidempotencymiddleware)
- [`Pam\Api\Middleware\ObserveRequestMiddleware`](#pamapimiddlewareobserverequestmiddleware)
- [`Pam\Api\Middleware\QueryBudgetMiddleware`](#pamapimiddlewarequerybudgetmiddleware)
- [`Pam\Api\Middleware\RateLimitMiddleware`](#pamapimiddlewareratelimitmiddleware)
- [`Pam\Api\Middleware\ResolveTenantMiddleware`](#pamapimiddlewareresolvetenantmiddleware)
- [`Pam\Api\Middleware\ResponseCacheMiddleware`](#pamapimiddlewareresponsecachemiddleware)
- [`Pam\Api\Middleware\SecurityHeadersMiddleware`](#pamapimiddlewaresecurityheadersmiddleware)
- [`Pam\Api\Middleware\TransactionalMiddleware`](#pamapimiddlewaretransactionalmiddleware)
- [`Pam\Api\Observability\RequestObservation`](#pamapiobservabilityrequestobservation)
- [`Pam\Api\Observability\RequestObserver`](#pamapiobservabilityrequestobserver)
- [`Pam\Api\OpenApi\ClientGenerator`](#pamapiopenapiclientgenerator)
- [`Pam\Api\OpenApi\ClientLanguage`](#pamapiopenapiclientlanguage)
- [`Pam\Api\OpenApi\CompatibilityChange`](#pamapiopenapicompatibilitychange)
- [`Pam\Api\OpenApi\CompatibilityChangeCode`](#pamapiopenapicompatibilitychangecode)
- [`Pam\Api\OpenApi\CompatibilityChecker`](#pamapiopenapicompatibilitychecker)
- [`Pam\Api\OpenApi\OpenApiGenerator`](#pamapiopenapiopenapigenerator)
- [`Pam\Api\PackageDiscovery`](#pamapipackagediscovery)
- [`Pam\Api\PendingRoute`](#pamapipendingroute)
- [`Pam\Api\Pipeline`](#pamapipipeline)
- [`Pam\Api\Profiler\MemoryProfiler`](#pamapiprofilermemoryprofiler)
- [`Pam\Api\Profiler\ProfilerMode`](#pamapiprofilerprofilermode)
- [`Pam\Api\Profiler\RequestProfile`](#pamapiprofilerrequestprofile)
- [`Pam\Api\RateLimit\MemoryRateLimitStore`](#pamapiratelimitmemoryratelimitstore)
- [`Pam\Api\RateLimit\RateLimitDecision`](#pamapiratelimitratelimitdecision)
- [`Pam\Api\RateLimit\RateLimitStore`](#pamapiratelimitratelimitstore)
- [`Pam\Api\Resilience\CircuitBreaker`](#pamapiresiliencecircuitbreaker)
- [`Pam\Api\Resilience\CircuitOpenException`](#pamapiresiliencecircuitopenexception)
- [`Pam\Api\Resilience\CircuitState`](#pamapiresiliencecircuitstate)
- [`Pam\Api\Resilience\RetryPolicy`](#pamapiresilienceretrypolicy)
- [`Pam\Api\Route`](#pamapiroute)
- [`Pam\Api\RouteConstraint`](#pamapirouteconstraint)
- [`Pam\Api\RouteRegistrar`](#pamapirouteregistrar)
- [`Pam\Api\Router`](#pamapirouter)
- [`Pam\Api\RoutingResult`](#pamapiroutingresult)
- [`Pam\Api\RoutingResultType`](#pamapiroutingresulttype)
- [`Pam\Api\Routing\RouteBindable`](#pamapiroutingroutebindable)
- [`Pam\Api\Runtime\Deadline`](#pamapiruntimedeadline)
- [`Pam\Api\Tenancy\Tenant`](#pamapitenancytenant)
- [`Pam\Api\Tenancy\TenantContext`](#pamapitenancytenantcontext)
- [`Pam\Api\Tenancy\TenantResolver`](#pamapitenancytenantresolver)
- [`Pam\Api\Testing\TestClient`](#pamapitestingtestclient)
- [`Pam\Api\Testing\TestResponse`](#pamapitestingtestresponse)
- [`Pam\Api\Transactions\TransactionManager`](#pamapitransactionstransactionmanager)
- [`Pam\Api\Validation\DtoHydrator`](#pamapivalidationdtohydrator)
- [`Pam\Api\Validation\EnumRule`](#pamapivalidationenumrule)
- [`Pam\Api\Validation\FormRequest`](#pamapivalidationformrequest)
- [`Pam\Api\Validation\Rule`](#pamapivalidationrule)
- [`Pam\Api\Validation\ValidationException`](#pamapivalidationvalidationexception)
- [`Pam\Api\Validation\ValidationRule`](#pamapivalidationvalidationrule)
- [`Pam\App`](#pamapp)

## `Pam\Api\Auth\AuthContext`

```php
final readonly class Pam\Api\Auth\AuthContext
```

### Properties

- `public readonly Pam\Api\Auth\Principal $principal`

### Methods

- `public __construct(Pam\Api\Auth\Principal $principal): mixed`

## `Pam\Api\Auth\Authenticator`

```php
abstract interface Pam\Api\Auth\Authenticator
```

### Methods

- `public authenticate(Pam\Http\Request $request): ?Pam\Api\Auth\Principal`

## `Pam\Api\Auth\BearerTokenAuthenticator`

```php
final readonly class Pam\Api\Auth\BearerTokenAuthenticator implements Pam\Api\Auth\Authenticator
```

### Methods

- `public __construct(Pam\Api\Auth\HmacTokenCodec $tokens, ?Pam\Api\Auth\TokenRevocationStore $revocations = null, ?callable $clock = null): mixed`
- `public authenticate(Pam\Http\Request $request): ?Pam\Api\Auth\Principal`

## `Pam\Api\Auth\HmacTokenCodec`

```php
final readonly class Pam\Api\Auth\HmacTokenCodec
```

### Methods

- `public __construct(string $secret, string $issuer, string $audience, int $leewaySeconds = 5, ?callable $clock = null, string $keyIdentifier = "primary", array $verificationKeys = []): mixed`
- `public issue(string $subject, array $abilities, int $ttlSeconds = 900): string`
- `public verify(string $token): ?Pam\Api\Auth\TokenPrincipal`

## `Pam\Api\Auth\MemoryTokenRevocationStore`

```php
final class Pam\Api\Auth\MemoryTokenRevocationStore implements Pam\Api\Auth\TokenRevocationStore
```

### Methods

- `public __construct(int $maximumEntries = 10000): mixed`
- `public isRevoked(string $tokenIdentifier, int $now): bool`
- `public revoke(string $tokenIdentifier, int $expiresAt): void`

## `Pam\Api\Auth\Principal`

```php
abstract interface Pam\Api\Auth\Principal
```

### Methods

- `public can(string $ability): bool`
- `public identifier(): string`

## `Pam\Api\Auth\TokenPrincipal`

```php
final readonly class Pam\Api\Auth\TokenPrincipal implements Pam\Api\Auth\Principal
```

### Properties

- `public readonly array $abilities`
- `public readonly string $tokenIdentifier`

### Methods

- `public __construct(string $subject, array $abilities, string $tokenIdentifier): mixed`
- `public can(string $ability): bool`
- `public identifier(): string`

## `Pam\Api\Auth\TokenRevocationStore`

```php
abstract interface Pam\Api\Auth\TokenRevocationStore
```

### Methods

- `public isRevoked(string $tokenIdentifier, int $now): bool`
- `public revoke(string $tokenIdentifier, int $expiresAt): void`

## `Pam\Api\Cache\CacheRecord`

```php
final readonly class Pam\Api\Cache\CacheRecord
```

### Properties

- `public readonly int $expiresAt`
- `public readonly Pam\Api\Http\ResponseSnapshot $response`

### Methods

- `public __construct(Pam\Api\Http\ResponseSnapshot $response, int $expiresAt): mixed`

## `Pam\Api\Cache\MemoryResponseCacheStore`

```php
final class Pam\Api\Cache\MemoryResponseCacheStore implements Pam\Api\Cache\ResponseCacheStore
```

### Methods

- `public __construct(int $maximumRecords = 10000): mixed`
- `public forget(string $key): void`
- `public get(string $key, int $now): ?Pam\Api\Cache\CacheRecord`
- `public put(string $key, Pam\Api\Cache\CacheRecord $record): void`

## `Pam\Api\Cache\ResponseCacheStore`

```php
abstract interface Pam\Api\Cache\ResponseCacheStore
```

### Methods

- `public forget(string $key): void`
- `public get(string $key, int $now): ?Pam\Api\Cache\CacheRecord`
- `public put(string $key, Pam\Api\Cache\CacheRecord $record): void`

## `Pam\Api\CallableRequestHandler`

```php
final readonly class Pam\Api\CallableRequestHandler implements Pam\Contracts\Http\RequestHandlerInterface
```

### Methods

- `public __construct(callable $handler): mixed`
- `public handle(Pam\Http\Request $request, Pam\Http\Response $response): Pam\Http\Response`

## `Pam\Api\Config\ConfigDefinition`

```php
final readonly class Pam\Api\Config\ConfigDefinition
```

### Properties

- `public readonly bool|float|int|null|string $default`
- `public readonly string $environment`
- `public readonly string $key`
- `public readonly bool $required`
- `public readonly bool $sensitive`
- `public readonly Pam\Api\Config\ConfigType $type`

### Methods

- `public __construct(string $key, string $environment, Pam\Api\Config\ConfigType $type = 1, bool $required = true, bool|float|int|null|string $default = null, bool $sensitive = false): mixed`

## `Pam\Api\Config\ConfigType`

```php
final enum Pam\Api\Config\ConfigType implements BackedEnum, UnitEnum: int
```

### Enum cases

- `String = 1`
- `Integer = 2`
- `Boolean = 3`
- `Float = 4`

### Constants

- `Boolean = 3`
- `Float = 4`
- `Integer = 2`
- `String = 1`

### Properties

- `public readonly string $name`
- `public readonly int $value`

### Methods

- `public static cases(): array`
- `public static from(int|string $value): static`
- `public static tryFrom(int|string $value): ?static`

## `Pam\Api\Config\Configuration`

```php
final readonly class Pam\Api\Config\Configuration
```

### Methods

- `public boolean(string $key): bool`
- `public float(string $key): float`
- `public static fromArray(array $definitions, array $environment): self`
- `public static fromEnvironment(array $definitions): self`
- `public integer(string $key): int`
- `public redacted(): array`
- `public string(string $key): string`

## `Pam\Api\Config\ConfigurationException`

```php
final class Pam\Api\Config\ConfigurationException extends RuntimeException implements Stringable, Throwable
```

### Properties

- `public readonly array $errors`

### Methods

- `public __construct(array $errors): mixed`

## `Pam\Api\ContainerMiddleware`

```php
final readonly class Pam\Api\ContainerMiddleware implements Pam\Contracts\Http\MiddlewareInterface
```

### Methods

- `public __construct(Pam\Api\Container\Container $container, string $middleware): mixed`
- `public process(Pam\Http\Request $request, Pam\Http\Response $response, Pam\Contracts\Http\RequestHandlerInterface $next): Pam\Http\Response`

## `Pam\Api\Container\Binding`

```php
final readonly class Pam\Api\Container\Binding
```

### Properties

- `public readonly Closure $factory`
- `public readonly Pam\Api\Container\BindingLifetime $lifetime`

### Methods

- `public __construct(callable $factory, Pam\Api\Container\BindingLifetime $lifetime): mixed`

## `Pam\Api\Container\BindingLifetime`

```php
final enum Pam\Api\Container\BindingLifetime implements BackedEnum, UnitEnum: int
```

### Enum cases

- `Transient = 1`
- `Singleton = 2`
- `Scoped = 3`

### Constants

- `Scoped = 3`
- `Singleton = 2`
- `Transient = 1`

### Properties

- `public readonly string $name`
- `public readonly int $value`

### Methods

- `public static cases(): array`
- `public static from(int|string $value): static`
- `public static tryFrom(int|string $value): ?static`

## `Pam\Api\Container\Container`

```php
final class Pam\Api\Container\Container
```

### Methods

- `public __construct(): mixed`
- `public beginScope(): void`
- `public bind(string $id, callable|null|string $factory = null): self`
- `public bindRoute(string $class, callable $resolver): self`
- `public call(array $callable, array $named = [], array $provided = []): mixed`
- `public diagnostics(): array`
- `public endScope(): void`
- `public get(string $id): mixed`
- `public instance(string $id, mixed $instance): self`
- `public scoped(string $id, callable|null|string $factory = null): self`
- `public scopedInstance(string $id, mixed $instance): self`
- `public scopedValue(string $id): mixed`
- `public singleton(string $id, callable|null|string $factory = null): self`

## `Pam\Api\Container\ContainerState`

```php
final enum Pam\Api\Container\ContainerState implements BackedEnum, UnitEnum: int
```

### Enum cases

- `Idle = 1`
- `RequestActive = 2`

### Constants

- `Idle = 1`
- `RequestActive = 2`

### Properties

- `public readonly string $name`
- `public readonly int $value`

### Methods

- `public static cases(): array`
- `public static from(int|string $value): static`
- `public static tryFrom(int|string $value): ?static`

## `Pam\Api\ControllerHandler`

```php
final readonly class Pam\Api\ControllerHandler
```

### Methods

- `public __construct(Pam\Api\Container\Container $container, string $controller, string $method): mixed`
- `public __invoke(Pam\Http\Request $request, Pam\Http\Response $response): mixed`

## `Pam\Api\Database\DatabaseConfig`

```php
final readonly class Pam\Api\Database\DatabaseConfig
```

### Properties

- `public readonly array $connections`
- `public readonly string $defaultConnection`

### Methods

- `public __construct(string $defaultConnection, array $connections): mixed`
- `public static fromEnvironment(): self`

## `Pam\Api\Database\DatabaseHealthCheck`

```php
final readonly class Pam\Api\Database\DatabaseHealthCheck implements Pam\Api\Health\HealthCheck
```

### Methods

- `public __construct(Pam\Api\Database\EloquentManager $eloquent, ?string $connection = null): mixed`
- `public check(): Pam\Api\Health\HealthResult`

## `Pam\Api\Database\EloquentManager`

```php
final readonly class Pam\Api\Database\EloquentManager implements Pam\Api\Transactions\TransactionManager
```

### Methods

- `public __construct(Pam\Api\Database\FiberConnectionResolver $connections): mixed`
- `public boot(): void`
- `public connection(?string $name = null): Illuminate\Database\Connection`
- `public events(): Illuminate\Events\Dispatcher`
- `public releaseCurrentRequest(): void`
- `public schema(?string $name = null): Illuminate\Database\Schema\Builder`
- `public transaction(callable $operation): mixed`

## `Pam\Api\Database\EloquentServiceProvider`

```php
final readonly class Pam\Api\Database\EloquentServiceProvider implements Pam\Contracts\Package\ServiceProviderInterface
```

### Methods

- `public __construct(Pam\Api\Database\DatabaseConfig $config): mixed`
- `public boot(Pam\Contracts\Http\ApplicationInterface $application): void`
- `public register(Pam\Contracts\Http\ApplicationInterface $application): void`

## `Pam\Api\Database\MigrationManager`

```php
final readonly class Pam\Api\Database\MigrationManager
```

### Methods

- `public __construct(Pam\Api\Database\FiberConnectionResolver $connections, ?string $connection = null, string $table = "migrations"): mixed`
- `public migrate(array|string $paths, bool $pretend = false): array`
- `public rollback(array|string $paths, int $steps = 0, bool $pretend = false): array`

## `Pam\Api\Database\QueryBudget`

```php
final readonly class Pam\Api\Database\QueryBudget
```

### Properties

- `public readonly int $maximumDuplicateQueries`
- `public readonly float $maximumElapsedMilliseconds`
- `public readonly int $maximumQueries`

### Methods

- `public __construct(int $maximumQueries = 100, float $maximumElapsedMilliseconds = 500, int $maximumDuplicateQueries = 10): mixed`

## `Pam\Api\Database\QueryBudgetExceededException`

```php
final class Pam\Api\Database\QueryBudgetExceededException extends RuntimeException implements Stringable, Throwable
```

### Properties

- `public readonly Pam\Api\Database\QueryReport $report`

### Methods

- `public __construct(Pam\Api\Database\QueryReport $report): mixed`

## `Pam\Api\Database\QueryBudgetViolation`

```php
final enum Pam\Api\Database\QueryBudgetViolation implements BackedEnum, UnitEnum: int
```

### Enum cases

- `QueryCount = 1`
- `ElapsedTime = 2`
- `DuplicateQuery = 3`

### Constants

- `DuplicateQuery = 3`
- `ElapsedTime = 2`
- `QueryCount = 1`

### Properties

- `public readonly string $name`
- `public readonly int $value`

### Methods

- `public static cases(): array`
- `public static from(int|string $value): static`
- `public static tryFrom(int|string $value): ?static`

## `Pam\Api\Database\QueryMonitor`

```php
final class Pam\Api\Database\QueryMonitor
```

### Methods

- `public __construct(Pam\Api\Database\FiberConnectionResolver $connections): mixed`
- `public begin(Pam\Api\Database\QueryBudget $budget): void`
- `public finish(): Pam\Api\Database\QueryReport`

## `Pam\Api\Database\QueryReport`

```php
final readonly class Pam\Api\Database\QueryReport
```

### Properties

- `public readonly int $count`
- `public readonly array $duplicates`
- `public readonly float $elapsedMilliseconds`
- `public readonly array $violations`

### Methods

- `public __construct(int $count, float $elapsedMilliseconds, array $duplicates, array $violations): mixed`
- `public withinBudget(): bool`

## `Pam\Api\Database\TenantModelGuard`

```php
final readonly class Pam\Api\Database\TenantModelGuard
```

### Methods

- `public __construct(Pam\Api\Container\Container $container): mixed`
- `public protect(string $model, string $column = "tenant_id"): void`

## `Pam\Api\Database\TenantScope`

```php
final readonly class Pam\Api\Database\TenantScope implements Illuminate\Database\Eloquent\Scope
```

### Methods

- `public __construct(Pam\Api\Container\Container $container, string $column = "tenant_id"): mixed`
- `public apply(Illuminate\Database\Eloquent\Builder $builder, Illuminate\Database\Eloquent\Model $model): void`
- `public assign(Illuminate\Database\Eloquent\Model $model): void`

## `Pam\Api\Events\EventDispatcher`

```php
abstract interface Pam\Api\Events\EventDispatcher
```

### Methods

- `public dispatch(object $event): object`

## `Pam\Api\Events\SyncEventDispatcher`

```php
final class Pam\Api\Events\SyncEventDispatcher implements Pam\Api\Events\EventDispatcher
```

### Methods

- `public __construct(Pam\Api\Container\Container $container): mixed`
- `public dispatch(object $event): object`
- `public listen(string $event, callable|string $listener): self`

## `Pam\Api\HandlerResolver`

```php
final readonly class Pam\Api\HandlerResolver
```

### Methods

- `public __construct(Pam\Api\Container\Container $container): mixed`
- `public resolve(array|callable|string $handler): Closure`

## `Pam\Api\Health\HealthCheck`

```php
abstract interface Pam\Api\Health\HealthCheck
```

### Methods

- `public check(): Pam\Api\Health\HealthResult`

## `Pam\Api\Health\HealthRegistry`

```php
final class Pam\Api\Health\HealthRegistry
```

### Methods

- `public add(string $name, Pam\Api\Health\HealthCheck $check): self`
- `public run(): array`

## `Pam\Api\Health\HealthResult`

```php
final readonly class Pam\Api\Health\HealthResult
```

### Properties

- `public readonly array $details`
- `public readonly Pam\Api\Health\HealthState $state`

### Methods

- `public __construct(Pam\Api\Health\HealthState $state, array $details = []): mixed`

## `Pam\Api\Health\HealthState`

```php
final enum Pam\Api\Health\HealthState implements BackedEnum, UnitEnum: int
```

### Enum cases

- `Healthy = 1`
- `Degraded = 2`
- `Unhealthy = 3`

### Constants

- `Degraded = 2`
- `Healthy = 1`
- `Unhealthy = 3`

### Properties

- `public readonly string $name`
- `public readonly int $value`

### Methods

- `public static cases(): array`
- `public static from(int|string $value): static`
- `public static tryFrom(int|string $value): ?static`

## `Pam\Api\Http\ClientIpResolver`

```php
final readonly class Pam\Api\Http\ClientIpResolver
```

### Methods

- `public __construct(array $trustedProxies = []): mixed`
- `public resolve(Pam\Http\Request $request): string`

## `Pam\Api\Http\HttpException`

```php
class Pam\Api\Http\HttpException extends RuntimeException implements Stringable, Throwable
```

### Properties

- `public readonly array $details`
- `public readonly Pam\Api\Http\ProblemCode $problemCode`
- `public readonly int $status`

### Methods

- `public __construct(int $status, Pam\Api\Http\ProblemCode $problemCode, string $message, array $details = [], ?Throwable $previous = null): mixed`

## `Pam\Api\Http\JsonResource`

```php
abstract readonly class Pam\Api\Http\JsonResource implements Pam\Api\Http\Responsable
```

### Methods

- `public __construct(mixed $resource, int $status = 200): mixed`
- `public static collection(iterable $resources): Pam\Api\Http\ResourceCollection`
- `public toArray(Pam\Http\Request $request): array`
- `public toResponse(Pam\Http\Request $request, Pam\Http\Response $response): Pam\Http\Response`

## `Pam\Api\Http\ProblemCode`

```php
final enum Pam\Api\Http\ProblemCode implements BackedEnum, UnitEnum: int
```

### Enum cases

- `ValidationFailed = 1`
- `Unauthenticated = 2`
- `Forbidden = 3`
- `NotFound = 4`
- `Conflict = 5`
- `RateLimited = 6`
- `Timeout = 7`
- `InternalError = 8`
- `MethodNotAllowed = 9`

### Constants

- `Conflict = 5`
- `Forbidden = 3`
- `InternalError = 8`
- `MethodNotAllowed = 9`
- `NotFound = 4`
- `RateLimited = 6`
- `Timeout = 7`
- `Unauthenticated = 2`
- `ValidationFailed = 1`

### Properties

- `public readonly string $name`
- `public readonly int $value`

### Methods

- `public static cases(): array`
- `public static from(int|string $value): static`
- `public static tryFrom(int|string $value): ?static`

## `Pam\Api\Http\ResourceCollection`

```php
final readonly class Pam\Api\Http\ResourceCollection implements Pam\Api\Http\Responsable
```

### Methods

- `public __construct(array $resources, array $meta = []): mixed`
- `public toResponse(Pam\Http\Request $request, Pam\Http\Response $response): Pam\Http\Response`
- `public withMeta(array $meta): self`

## `Pam\Api\Http\Responsable`

```php
abstract interface Pam\Api\Http\Responsable
```

### Methods

- `public toResponse(Pam\Http\Request $request, Pam\Http\Response $response): Pam\Http\Response`

## `Pam\Api\Http\ResponseSnapshot`

```php
final readonly class Pam\Api\Http\ResponseSnapshot
```

### Properties

- `public readonly string $body`
- `public readonly array $headers`
- `public readonly int $status`

### Methods

- `public __construct(int $status, array $headers, string $body): mixed`
- `public static capture(Pam\Http\Response $response): self`
- `public restore(Pam\Http\Response $response): Pam\Http\Response`

## `Pam\Api\Idempotency\IdempotencyRecord`

```php
final readonly class Pam\Api\Idempotency\IdempotencyRecord
```

### Properties

- `public readonly int $expiresAt`
- `public readonly string $fingerprint`
- `public readonly Pam\Api\Http\ResponseSnapshot $response`

### Methods

- `public __construct(string $fingerprint, Pam\Api\Http\ResponseSnapshot $response, int $expiresAt): mixed`

## `Pam\Api\Idempotency\IdempotencyStore`

```php
abstract interface Pam\Api\Idempotency\IdempotencyStore
```

### Methods

- `public get(string $key, int $now): ?Pam\Api\Idempotency\IdempotencyRecord`
- `public put(string $key, Pam\Api\Idempotency\IdempotencyRecord $record): void`

## `Pam\Api\Idempotency\MemoryIdempotencyStore`

```php
final class Pam\Api\Idempotency\MemoryIdempotencyStore implements Pam\Api\Idempotency\IdempotencyStore
```

### Methods

- `public __construct(int $maximumRecords = 10000): mixed`
- `public get(string $key, int $now): ?Pam\Api\Idempotency\IdempotencyRecord`
- `public put(string $key, Pam\Api\Idempotency\IdempotencyRecord $record): void`

## `Pam\Api\Jobs\JobCodec`

```php
final readonly class Pam\Api\Jobs\JobCodec
```

### Methods

- `public __construct(array $jobs): mixed`
- `public decode(string $json): Pam\Api\Jobs\SerializableJob`
- `public encode(Pam\Api\Jobs\SerializableJob $job): string`

## `Pam\Api\Jobs\JobDispatcher`

```php
abstract interface Pam\Api\Jobs\JobDispatcher
```

### Methods

- `public dispatch(object $job, int $maximumAttempts = 3, int $delaySeconds = 0): Pam\Api\Jobs\JobEnvelope`

## `Pam\Api\Jobs\JobEnvelope`

```php
final class Pam\Api\Jobs\JobEnvelope
```

### Properties

- `public int $attempts`
- `public int $availableAt`
- `public readonly string $id`
- `public readonly object $job`
- `public ?string $lastFailureClass`
- `public ?int $leaseUntil`
- `public readonly int $maximumAttempts`
- `public Pam\Api\Jobs\JobState $state`

### Methods

- `public __construct(string $id, object $job, int $maximumAttempts = 3, int $availableAt = 0): mixed`

## `Pam\Api\Jobs\JobHandler`

```php
abstract interface Pam\Api\Jobs\JobHandler
```

### Methods

- `public handle(object $job): Pam\Api\Jobs\JobOutcome`

## `Pam\Api\Jobs\JobOutcome`

```php
final enum Pam\Api\Jobs\JobOutcome implements BackedEnum, UnitEnum: int
```

### Enum cases

- `Complete = 1`
- `Retry = 2`
- `DeadLetter = 3`

### Constants

- `Complete = 1`
- `DeadLetter = 3`
- `Retry = 2`

### Properties

- `public readonly string $name`
- `public readonly int $value`

### Methods

- `public static cases(): array`
- `public static from(int|string $value): static`
- `public static tryFrom(int|string $value): ?static`

## `Pam\Api\Jobs\JobQueue`

```php
abstract interface Pam\Api\Jobs\JobQueue implements Pam\Api\Jobs\JobDispatcher
```

### Methods

- `public complete(Pam\Api\Jobs\JobEnvelope $envelope): void`
- `public deadLetter(Pam\Api\Jobs\JobEnvelope $envelope, ?Throwable $failure = null): void`
- `public release(Pam\Api\Jobs\JobEnvelope $envelope, int $availableAt, ?Throwable $failure = null): void`
- `public reserve(int $now, int $leaseSeconds): ?Pam\Api\Jobs\JobEnvelope`

## `Pam\Api\Jobs\JobRunResult`

```php
final enum Pam\Api\Jobs\JobRunResult implements BackedEnum, UnitEnum: int
```

### Enum cases

- `Idle = 1`
- `Completed = 2`
- `Retried = 3`
- `DeadLettered = 4`

### Constants

- `Completed = 2`
- `DeadLettered = 4`
- `Idle = 1`
- `Retried = 3`

### Properties

- `public readonly string $name`
- `public readonly int $value`

### Methods

- `public static cases(): array`
- `public static from(int|string $value): static`
- `public static tryFrom(int|string $value): ?static`

## `Pam\Api\Jobs\JobState`

```php
final enum Pam\Api\Jobs\JobState implements BackedEnum, UnitEnum: int
```

### Enum cases

- `Pending = 1`
- `Processing = 2`
- `Completed = 3`
- `Failed = 4`
- `DeadLetter = 5`

### Constants

- `Completed = 3`
- `DeadLetter = 5`
- `Failed = 4`
- `Pending = 1`
- `Processing = 2`

### Properties

- `public readonly string $name`
- `public readonly int $value`

### Methods

- `public static cases(): array`
- `public static from(int|string $value): static`
- `public static tryFrom(int|string $value): ?static`

## `Pam\Api\Jobs\JobWorker`

```php
final readonly class Pam\Api\Jobs\JobWorker
```

### Methods

- `public __construct(Pam\Api\Jobs\JobQueue $queue, Pam\Api\Jobs\JobHandler|callable $handler, int $leaseSeconds = 30, int $retryDelaySeconds = 5): mixed`
- `public runOne(?int $now = null): Pam\Api\Jobs\JobRunResult`

## `Pam\Api\Jobs\MemoryJobDispatcher`

```php
final class Pam\Api\Jobs\MemoryJobDispatcher implements Pam\Api\Jobs\JobDispatcher, Pam\Api\Jobs\JobQueue
```

### Methods

- `public __construct(int $maximumQueuedJobs = 10000): mixed`
- `public complete(Pam\Api\Jobs\JobEnvelope $envelope): void`
- `public deadLetter(Pam\Api\Jobs\JobEnvelope $envelope, ?Throwable $failure = null): void`
- `public deadLetters(): array`
- `public dispatch(object $job, int $maximumAttempts = 3, int $delaySeconds = 0): Pam\Api\Jobs\JobEnvelope`
- `public pending(): array`
- `public release(Pam\Api\Jobs\JobEnvelope $envelope, int $availableAt, ?Throwable $failure = null): void`
- `public reserve(int $now, int $leaseSeconds): ?Pam\Api\Jobs\JobEnvelope`

## `Pam\Api\Jobs\SerializableJob`

```php
abstract interface Pam\Api\Jobs\SerializableJob
```

### Methods

- `public static fromJobPayload(array $payload): self`
- `public static jobName(): string`
- `public toJobPayload(): array`

## `Pam\Api\Lifecycle\RequestLifecycleObserver`

```php
abstract interface Pam\Api\Lifecycle\RequestLifecycleObserver
```

### Methods

- `public finished(Pam\Http\Request $request, Pam\Http\Response $response, ?Throwable $failure): void`
- `public starting(Pam\Http\Request $request): void`

## `Pam\Api\Middleware\AuthenticateMiddleware`

```php
final readonly class Pam\Api\Middleware\AuthenticateMiddleware implements Pam\Contracts\Http\MiddlewareInterface
```

### Methods

- `public __construct(Pam\Api\Auth\Authenticator $authenticator, Pam\Api\Container\Container $container): mixed`
- `public process(Pam\Http\Request $request, Pam\Http\Response $response, Pam\Contracts\Http\RequestHandlerInterface $next): Pam\Http\Response`

## `Pam\Api\Middleware\AuthorizeMiddleware`

```php
final readonly class Pam\Api\Middleware\AuthorizeMiddleware implements Pam\Contracts\Http\MiddlewareInterface
```

### Methods

- `public __construct(Pam\Api\Container\Container $container, string $ability): mixed`
- `public process(Pam\Http\Request $request, Pam\Http\Response $response, Pam\Contracts\Http\RequestHandlerInterface $next): Pam\Http\Response`

## `Pam\Api\Middleware\CorsMiddleware`

```php
final readonly class Pam\Api\Middleware\CorsMiddleware implements Pam\Contracts\Http\MiddlewareInterface
```

### Methods

- `public __construct(array $origins, array $methods = ["GET","POST","PUT","PATCH","DELETE","OPTIONS"], array $headers = ["content-type","authorization"], bool $credentials = false, int $maxAge = 600): mixed`
- `public process(Pam\Http\Request $request, Pam\Http\Response $response, Pam\Contracts\Http\RequestHandlerInterface $next): Pam\Http\Response`

## `Pam\Api\Middleware\DeadlineMiddleware`

```php
final readonly class Pam\Api\Middleware\DeadlineMiddleware implements Pam\Contracts\Http\MiddlewareInterface
```

### Methods

- `public __construct(Pam\Api\Container\Container $container, float $seconds): mixed`
- `public process(Pam\Http\Request $request, Pam\Http\Response $response, Pam\Contracts\Http\RequestHandlerInterface $next): Pam\Http\Response`

## `Pam\Api\Middleware\EloquentLifecycleMiddleware`

```php
final readonly class Pam\Api\Middleware\EloquentLifecycleMiddleware implements Pam\Contracts\Http\MiddlewareInterface
```

### Methods

- `public __construct(Pam\Api\Database\EloquentManager $eloquent): mixed`
- `public process(Pam\Http\Request $request, Pam\Http\Response $response, Pam\Contracts\Http\RequestHandlerInterface $next): Pam\Http\Response`

## `Pam\Api\Middleware\IdempotencyMiddleware`

```php
final readonly class Pam\Api\Middleware\IdempotencyMiddleware implements Pam\Contracts\Http\MiddlewareInterface
```

### Methods

- `public __construct(Pam\Api\Idempotency\IdempotencyStore $store, int $ttlSeconds = 86400, bool $required = true): mixed`
- `public process(Pam\Http\Request $request, Pam\Http\Response $response, Pam\Contracts\Http\RequestHandlerInterface $next): Pam\Http\Response`

## `Pam\Api\Middleware\ObserveRequestMiddleware`

```php
final readonly class Pam\Api\Middleware\ObserveRequestMiddleware implements Pam\Contracts\Http\MiddlewareInterface
```

### Methods

- `public __construct(Pam\Api\Observability\RequestObserver $observer, ?Pam\Api\Container\Container $container = null): mixed`
- `public process(Pam\Http\Request $request, Pam\Http\Response $response, Pam\Contracts\Http\RequestHandlerInterface $next): Pam\Http\Response`

## `Pam\Api\Middleware\QueryBudgetMiddleware`

```php
final readonly class Pam\Api\Middleware\QueryBudgetMiddleware implements Pam\Contracts\Http\MiddlewareInterface
```

### Methods

- `public __construct(Pam\Api\Database\QueryMonitor $monitor, Pam\Api\Database\QueryBudget $budget = {"@object":"Pam\\Api\\Database\\QueryBudget","properties":{"maximumDuplicateQueries":10,"maximumElapsedMilliseconds":500,"maximumQueries":100}}, bool $failOnViolation = false): mixed`
- `public process(Pam\Http\Request $request, Pam\Http\Response $response, Pam\Contracts\Http\RequestHandlerInterface $next): Pam\Http\Response`

## `Pam\Api\Middleware\RateLimitMiddleware`

```php
final class Pam\Api\Middleware\RateLimitMiddleware implements Pam\Contracts\Http\MiddlewareInterface
```

### Methods

- `public __construct(int $requestsPerSecond, int $burst = 0, int $maxBuckets = 65536, float $idleTtlSeconds = 300, ?Pam\Api\RateLimit\RateLimitStore $store = null, ?callable $keyResolver = null): mixed`
- `public process(Pam\Http\Request $request, Pam\Http\Response $response, Pam\Contracts\Http\RequestHandlerInterface $next): Pam\Http\Response`

## `Pam\Api\Middleware\ResolveTenantMiddleware`

```php
final readonly class Pam\Api\Middleware\ResolveTenantMiddleware implements Pam\Contracts\Http\MiddlewareInterface
```

### Methods

- `public __construct(Pam\Api\Tenancy\TenantResolver $resolver, Pam\Api\Container\Container $container, bool $required = true): mixed`
- `public process(Pam\Http\Request $request, Pam\Http\Response $response, Pam\Contracts\Http\RequestHandlerInterface $next): Pam\Http\Response`

## `Pam\Api\Middleware\ResponseCacheMiddleware`

```php
final readonly class Pam\Api\Middleware\ResponseCacheMiddleware implements Pam\Contracts\Http\MiddlewareInterface
```

### Methods

- `public __construct(Pam\Api\Cache\ResponseCacheStore $store, int $ttlSeconds = 60, ?callable $keyResolver = null): mixed`
- `public process(Pam\Http\Request $request, Pam\Http\Response $response, Pam\Contracts\Http\RequestHandlerInterface $next): Pam\Http\Response`

## `Pam\Api\Middleware\SecurityHeadersMiddleware`

```php
final readonly class Pam\Api\Middleware\SecurityHeadersMiddleware implements Pam\Contracts\Http\MiddlewareInterface
```

### Methods

- `public __construct(bool $hsts = true): mixed`
- `public process(Pam\Http\Request $request, Pam\Http\Response $response, Pam\Contracts\Http\RequestHandlerInterface $next): Pam\Http\Response`

## `Pam\Api\Middleware\TransactionalMiddleware`

```php
final readonly class Pam\Api\Middleware\TransactionalMiddleware implements Pam\Contracts\Http\MiddlewareInterface
```

### Methods

- `public __construct(Pam\Api\Transactions\TransactionManager $transactions): mixed`
- `public process(Pam\Http\Request $request, Pam\Http\Response $response, Pam\Contracts\Http\RequestHandlerInterface $next): Pam\Http\Response`

## `Pam\Api\Observability\RequestObservation`

```php
final readonly class Pam\Api\Observability\RequestObservation
```

### Properties

- `public readonly float $durationSeconds`
- `public readonly ?string $exceptionClass`
- `public readonly string $method`
- `public readonly string $route`
- `public readonly int $status`

### Methods

- `public __construct(string $method, string $route, int $status, float $durationSeconds, ?string $exceptionClass = null): mixed`

## `Pam\Api\Observability\RequestObserver`

```php
abstract interface Pam\Api\Observability\RequestObserver
```

### Methods

- `public record(Pam\Api\Observability\RequestObservation $observation): void`

## `Pam\Api\OpenApi\ClientGenerator`

```php
final readonly class Pam\Api\OpenApi\ClientGenerator
```

### Methods

- `public __construct(array $document): mixed`
- `public generate(Pam\Api\OpenApi\ClientLanguage $language): string`

## `Pam\Api\OpenApi\ClientLanguage`

```php
final enum Pam\Api\OpenApi\ClientLanguage implements BackedEnum, UnitEnum: int
```

### Enum cases

- `TypeScript = 1`
- `Kotlin = 2`
- `Swift = 3`

### Constants

- `Kotlin = 2`
- `Swift = 3`
- `TypeScript = 1`

### Properties

- `public readonly string $name`
- `public readonly int $value`

### Methods

- `public static cases(): array`
- `public static from(int|string $value): static`
- `public static tryFrom(int|string $value): ?static`

## `Pam\Api\OpenApi\CompatibilityChange`

```php
final readonly class Pam\Api\OpenApi\CompatibilityChange
```

### Properties

- `public readonly Pam\Api\OpenApi\CompatibilityChangeCode $code`
- `public readonly string $location`
- `public readonly string $message`

### Methods

- `public __construct(Pam\Api\OpenApi\CompatibilityChangeCode $code, string $location, string $message): mixed`

## `Pam\Api\OpenApi\CompatibilityChangeCode`

```php
final enum Pam\Api\OpenApi\CompatibilityChangeCode implements BackedEnum, UnitEnum: int
```

### Enum cases

- `PathRemoved = 1`
- `OperationRemoved = 2`
- `RequiredInputAdded = 3`

### Constants

- `OperationRemoved = 2`
- `PathRemoved = 1`
- `RequiredInputAdded = 3`

### Properties

- `public readonly string $name`
- `public readonly int $value`

### Methods

- `public static cases(): array`
- `public static from(int|string $value): static`
- `public static tryFrom(int|string $value): ?static`

## `Pam\Api\OpenApi\CompatibilityChecker`

```php
final class Pam\Api\OpenApi\CompatibilityChecker
```

### Methods

- `public breakingChanges(array $previous, array $current): array`

## `Pam\Api\OpenApi\OpenApiGenerator`

```php
final readonly class Pam\Api\OpenApi\OpenApiGenerator
```

### Methods

- `public __construct(Pam\Api\Router $router, string $title = "PAM API", string $version = "1.0.0"): mixed`
- `public client(Pam\Api\OpenApi\ClientLanguage $language): string`
- `public generate(): array`
- `public toJson(): string`

## `Pam\Api\PackageDiscovery`

```php
final class Pam\Api\PackageDiscovery
```

### Methods

- `public static providers(string $projectRoot): array`

## `Pam\Api\PendingRoute`

```php
final readonly class Pam\Api\PendingRoute
```

### Methods

- `public __construct(Pam\App $app, Pam\Api\Router $router, Pam\Api\Route $route): mixed`
- `public definition(): Pam\Api\Route`
- `public delete(string $path, array|callable|string $handler): self`
- `public get(string $path, array|callable|string $handler): self`
- `public input(string $request): self`
- `public listen(int $port, string $host = "127.0.0.1", array $options = []): void`
- `public middleware(callable|object|string $middleware): self`
- `public name(string $name): self`
- `public output(string $resource): self`
- `public patch(string $path, array|callable|string $handler): self`
- `public post(string $path, array|callable|string $handler): self`
- `public put(string $path, array|callable|string $handler): self`
- `public summary(string $summary): self`
- `public tags(array $tags): self`
- `public where(string $parameter, Pam\Api\RouteConstraint|string $constraint): self`

## `Pam\Api\Pipeline`

```php
final class Pam\Api\Pipeline implements Pam\Contracts\Http\RequestHandlerInterface
```

### Methods

- `public __construct(array $middleware, Pam\Contracts\Http\RequestHandlerInterface $destination): mixed`
- `public handle(Pam\Http\Request $request, Pam\Http\Response $response): Pam\Http\Response`

## `Pam\Api\Profiler\MemoryProfiler`

```php
final class Pam\Api\Profiler\MemoryProfiler implements Pam\Api\Lifecycle\RequestLifecycleObserver
```

### Methods

- `public __construct(Pam\Api\Profiler\ProfilerMode $mode = 1, int $maximumProfiles = 500): mixed`
- `public find(string $token): ?Pam\Api\Profiler\RequestProfile`
- `public finished(Pam\Http\Request $request, Pam\Http\Response $response, ?Throwable $failure): void`
- `public recent(int $limit = 50): array`
- `public starting(Pam\Http\Request $request): void`

## `Pam\Api\Profiler\ProfilerMode`

```php
final enum Pam\Api\Profiler\ProfilerMode implements BackedEnum, UnitEnum: int
```

### Enum cases

- `Disabled = 1`
- `Development = 2`
- `Testing = 3`

### Constants

- `Development = 2`
- `Disabled = 1`
- `Testing = 3`

### Properties

- `public readonly string $name`
- `public readonly int $value`

### Methods

- `public static cases(): array`
- `public static from(int|string $value): static`
- `public static tryFrom(int|string $value): ?static`

## `Pam\Api\Profiler\RequestProfile`

```php
final readonly class Pam\Api\Profiler\RequestProfile
```

### Properties

- `public readonly float $durationMilliseconds`
- `public readonly ?string $failureClass`
- `public readonly int $memoryDeltaBytes`
- `public readonly string $method`
- `public readonly string $path`
- `public readonly int $statusCode`
- `public readonly string $token`

### Methods

- `public __construct(string $token, string $method, string $path, int $statusCode, float $durationMilliseconds, int $memoryDeltaBytes, ?string $failureClass): mixed`

## `Pam\Api\RateLimit\MemoryRateLimitStore`

```php
final class Pam\Api\RateLimit\MemoryRateLimitStore implements Pam\Api\RateLimit\RateLimitStore
```

### Methods

- `public __construct(int $maxBuckets = 65536, float $idleTtlSeconds = 300): mixed`
- `public consume(string $key, int $requestsPerSecond, int $capacity, float $now): Pam\Api\RateLimit\RateLimitDecision`

## `Pam\Api\RateLimit\RateLimitDecision`

```php
final readonly class Pam\Api\RateLimit\RateLimitDecision
```

### Properties

- `public readonly bool $allowed`
- `public readonly int $limit`
- `public readonly int $remaining`
- `public readonly int $retryAfterSeconds`

### Methods

- `public __construct(bool $allowed, int $limit, int $remaining, int $retryAfterSeconds = 0): mixed`

## `Pam\Api\RateLimit\RateLimitStore`

```php
abstract interface Pam\Api\RateLimit\RateLimitStore
```

### Methods

- `public consume(string $key, int $requestsPerSecond, int $capacity, float $now): Pam\Api\RateLimit\RateLimitDecision`

## `Pam\Api\Resilience\CircuitBreaker`

```php
final class Pam\Api\Resilience\CircuitBreaker
```

### Methods

- `public __construct(int $failureThreshold = 5, int $cooldownSeconds = 30): mixed`
- `public call(callable $operation): mixed`
- `public state(): Pam\Api\Resilience\CircuitState`

## `Pam\Api\Resilience\CircuitOpenException`

```php
final class Pam\Api\Resilience\CircuitOpenException extends RuntimeException implements Stringable, Throwable
```

## `Pam\Api\Resilience\CircuitState`

```php
final enum Pam\Api\Resilience\CircuitState implements BackedEnum, UnitEnum: int
```

### Enum cases

- `Closed = 1`
- `Open = 2`
- `HalfOpen = 3`

### Constants

- `Closed = 1`
- `HalfOpen = 3`
- `Open = 2`

### Properties

- `public readonly string $name`
- `public readonly int $value`

### Methods

- `public static cases(): array`
- `public static from(int|string $value): static`
- `public static tryFrom(int|string $value): ?static`

## `Pam\Api\Resilience\RetryPolicy`

```php
final readonly class Pam\Api\Resilience\RetryPolicy
```

### Methods

- `public __construct(int $attempts = 3, int $initialDelayMilliseconds = 10, ?callable $when = null): mixed`
- `public run(callable $operation): mixed`

## `Pam\Api\Route`

```php
final class Pam\Api\Route
```

### Properties

- `public Closure $handler`
- `public ?string $input`
- `public string $method`
- `public array $middleware`
- `public ?string $name`
- `public ?string $output`
- `public readonly array $parameterNames`
- `public string $path`
- `public string $pattern`
- `public mixed $sourceHandler`
- `public ?string $summary`
- `public array $tags`

### Methods

- `public __construct(string $method, string $path, callable $handler, string $pattern, array $parameterNames): mixed`

## `Pam\Api\RouteConstraint`

```php
final enum Pam\Api\RouteConstraint implements BackedEnum, UnitEnum: int
```

### Enum cases

- `Integer = 1`
- `Uuid = 2`
- `Ulid = 3`
- `Slug = 4`
- `Alpha = 5`
- `AlphaNumeric = 6`

### Constants

- `Alpha = 5`
- `AlphaNumeric = 6`
- `Integer = 1`
- `Slug = 4`
- `Ulid = 3`
- `Uuid = 2`

### Properties

- `public readonly string $name`
- `public readonly int $value`

### Methods

- `public static cases(): array`
- `public static from(int|string $value): static`
- `public pattern(): string`
- `public static tryFrom(int|string $value): ?static`

## `Pam\Api\RouteRegistrar`

```php
final class Pam\Api\RouteRegistrar
```

### Methods

- `public __construct(Pam\App $app, string $prefix = ""): mixed`
- `public apiResource(string $path, string $controller): void`
- `public delete(string $path, array|callable|string $handler): Pam\Api\PendingRoute`
- `public get(string $path, array|callable|string $handler): Pam\Api\PendingRoute`
- `public group(callable $routes): void`
- `public head(string $path, array|callable|string $handler): Pam\Api\PendingRoute`
- `public middleware(Pam\Contracts\Http\MiddlewareInterface|array|callable|string $middleware): self`
- `public options(string $path, array|callable|string $handler): Pam\Api\PendingRoute`
- `public patch(string $path, array|callable|string $handler): Pam\Api\PendingRoute`
- `public post(string $path, array|callable|string $handler): Pam\Api\PendingRoute`
- `public prefix(string $prefix): self`
- `public put(string $path, array|callable|string $handler): Pam\Api\PendingRoute`

## `Pam\Api\Router`

```php
final class Pam\Api\Router
```

### Methods

- `public __construct(?Pam\Api\Container\Container $container = null, int $maximumRoutes = 10000): mixed`
- `public add(string $method, string $path, callable $handler): self`
- `public constrain(Pam\Api\Route $route, string $parameter, Pam\Api\RouteConstraint|string $constraint): void`
- `public container(): Pam\Api\Container\Container`
- `public match(string $method, string $path): Pam\Api\RoutingResult`
- `public name(Pam\Api\Route $route, string $name): void`
- `public register(string $method, string $path, callable $handler): Pam\Api\Route`
- `public routes(): array`

## `Pam\Api\RoutingResult`

```php
final readonly class Pam\Api\RoutingResult
```

### Properties

- `public readonly array $allowedMethods`
- `public readonly array $parameters`
- `public readonly ?Pam\Api\Route $route`
- `public readonly Pam\Api\RoutingResultType $type`

### Methods

- `public __construct(Pam\Api\RoutingResultType $type, ?Pam\Api\Route $route = null, array $parameters = [], array $allowedMethods = []): mixed`

## `Pam\Api\RoutingResultType`

```php
final enum Pam\Api\RoutingResultType implements BackedEnum, UnitEnum: int
```

### Enum cases

- `Found = 1`
- `MethodNotAllowed = 2`
- `NotFound = 3`

### Constants

- `Found = 1`
- `MethodNotAllowed = 2`
- `NotFound = 3`

### Properties

- `public readonly string $name`
- `public readonly int $value`

### Methods

- `public static cases(): array`
- `public static from(int|string $value): static`
- `public static tryFrom(int|string $value): ?static`

## `Pam\Api\Routing\RouteBindable`

```php
abstract interface Pam\Api\Routing\RouteBindable
```

### Methods

- `public static resolveRouteBinding(string $value): static`

## `Pam\Api\Runtime\Deadline`

```php
final readonly class Pam\Api\Runtime\Deadline
```

### Methods

- `public __construct(float $seconds): mixed`
- `public expired(): bool`
- `public throwIfExpired(): void`

## `Pam\Api\Tenancy\Tenant`

```php
abstract interface Pam\Api\Tenancy\Tenant
```

### Methods

- `public identifier(): string`

## `Pam\Api\Tenancy\TenantContext`

```php
final readonly class Pam\Api\Tenancy\TenantContext
```

### Properties

- `public readonly Pam\Api\Tenancy\Tenant $tenant`

### Methods

- `public __construct(Pam\Api\Tenancy\Tenant $tenant): mixed`

## `Pam\Api\Tenancy\TenantResolver`

```php
abstract interface Pam\Api\Tenancy\TenantResolver
```

### Methods

- `public resolve(Pam\Http\Request $request): ?Pam\Api\Tenancy\Tenant`

## `Pam\Api\Testing\TestClient`

```php
final readonly class Pam\Api\Testing\TestClient
```

### Methods

- `public __construct(Pam\App $app): mixed`
- `public get(string $path, array $headers = []): Pam\Api\Testing\TestResponse`
- `public postJson(string $path, array $payload, array $headers = []): Pam\Api\Testing\TestResponse`
- `public request(string $method, string $path, array $query = [], array $headers = [], string $body = ""): Pam\Api\Testing\TestResponse`

## `Pam\Api\Testing\TestResponse`

```php
final readonly class Pam\Api\Testing\TestResponse
```

### Methods

- `public __construct(array $response): mixed`
- `public assertHeader(string $name, ?string $expected = null): self`
- `public assertJson(array $expected): self`
- `public assertJsonPath(string $path, mixed $expected): self`
- `public assertStatus(int $expected): self`
- `public assertSuccessful(): self`
- `public body(): string`
- `public header(string $name): ?string`
- `public json(): mixed`
- `public status(): int`

## `Pam\Api\Transactions\TransactionManager`

```php
abstract interface Pam\Api\Transactions\TransactionManager
```

### Methods

- `public transaction(callable $operation): mixed`

## `Pam\Api\Validation\DtoHydrator`

```php
final class Pam\Api\Validation\DtoHydrator
```

### Methods

- `public hydrate(string $class, array $data): object`

## `Pam\Api\Validation\EnumRule`

```php
final readonly class Pam\Api\Validation\EnumRule implements Pam\Api\Validation\ValidationRule
```

### Methods

- `public __construct(string $enum): mixed`
- `public validate(string $field, mixed $value): ?string`

## `Pam\Api\Validation\FormRequest`

```php
abstract class Pam\Api\Validation\FormRequest
```

### Methods

- `public __construct(Pam\Http\Request $request): mixed`
- `public authorize(): bool`
- `public dto(string $class): object`
- `public input(string $key, mixed $default = null): mixed`
- `public rules(): array`
- `public validated(): array`

## `Pam\Api\Validation\Rule`

```php
final class Pam\Api\Validation\Rule
```

### Methods

- `public static enum(string $enum): Pam\Api\Validation\EnumRule`

## `Pam\Api\Validation\ValidationException`

```php
final class Pam\Api\Validation\ValidationException extends Pam\Api\Http\HttpException implements Stringable, Throwable
```

### Properties

- `public readonly array $errors`

### Methods

- `public __construct(array $errors): mixed`

## `Pam\Api\Validation\ValidationRule`

```php
abstract interface Pam\Api\Validation\ValidationRule
```

### Methods

- `public validate(string $field, mixed $value): ?string`

## `Pam\App`

```php
final class Pam\App implements Pam\Contracts\Http\ApplicationInterface
```

### Methods

- `public __construct(bool $discoverPackages = true, ?Pam\Api\Container\Container $container = null): mixed`
- `public boot(): self`
- `public container(): Pam\Api\Container\Container`
- `public delete(string $path, array|callable|string $handler): Pam\Api\PendingRoute`
- `public get(string $path, array|callable|string $handler): Pam\Api\PendingRoute`
- `public group(callable $routes): void`
- `public handle(Pam\Http\Request $request, Pam\Http\Response $response): Pam\Http\Response`
- `public handler(callable|object $handler): self`
- `public head(string $path, array|callable|string $handler): Pam\Api\PendingRoute`
- `public listen(int $port, string $host = "127.0.0.1", array $options = []): void`
- `public middleware(callable|object|string $middleware): self`
- `public observe(Pam\Api\Lifecycle\RequestLifecycleObserver $observer): self`
- `public onError(callable $handler): self`
- `public openApi(string $title = "PAM API", string $version = "1.0.0"): Pam\Api\OpenApi\OpenApiGenerator`
- `public options(string $path, array|callable|string $handler): Pam\Api\PendingRoute`
- `public patch(string $path, array|callable|string $handler): Pam\Api\PendingRoute`
- `public post(string $path, array|callable|string $handler): Pam\Api\PendingRoute`
- `public prefix(string $prefix): Pam\Api\RouteRegistrar`
- `public provider(Pam\Contracts\Package\ServiceProviderInterface $provider): self`
- `public put(string $path, array|callable|string $handler): Pam\Api\PendingRoute`
- `public route(string $method, string $path, array|callable|string $handler): self`
- `public transport(Pam\Contracts\Transport\TransportProviderInterface $provider): self`
- `public transports(): array`
