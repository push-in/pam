# Nimbus public reference

> Generated from `api-surface.json` by `bin/api-reference`. Do not edit manually.

Every symbol below is protected by the Nimbus Semantic Versioning gate.

## Symbols

- [`Nimbus\App`](#nimbusapp)
- [`Nimbus\Auth\AuthContext`](#nimbusauthauthcontext)
- [`Nimbus\Auth\Authenticator`](#nimbusauthauthenticator)
- [`Nimbus\Auth\BearerTokenAuthenticator`](#nimbusauthbearertokenauthenticator)
- [`Nimbus\Auth\HmacTokenCodec`](#nimbusauthhmactokencodec)
- [`Nimbus\Auth\MemoryTokenRevocationStore`](#nimbusauthmemorytokenrevocationstore)
- [`Nimbus\Auth\Principal`](#nimbusauthprincipal)
- [`Nimbus\Auth\TokenPrincipal`](#nimbusauthtokenprincipal)
- [`Nimbus\Auth\TokenRevocationStore`](#nimbusauthtokenrevocationstore)
- [`Nimbus\Cache\CacheRecord`](#nimbuscachecacherecord)
- [`Nimbus\Cache\MemoryResponseCacheStore`](#nimbuscachememoryresponsecachestore)
- [`Nimbus\Cache\ResponseCacheStore`](#nimbuscacheresponsecachestore)
- [`Nimbus\CallableRequestHandler`](#nimbuscallablerequesthandler)
- [`Nimbus\Config\ConfigDefinition`](#nimbusconfigconfigdefinition)
- [`Nimbus\Config\ConfigType`](#nimbusconfigconfigtype)
- [`Nimbus\Config\Configuration`](#nimbusconfigconfiguration)
- [`Nimbus\Config\ConfigurationException`](#nimbusconfigconfigurationexception)
- [`Nimbus\ContainerMiddleware`](#nimbuscontainermiddleware)
- [`Nimbus\Container\Binding`](#nimbuscontainerbinding)
- [`Nimbus\Container\BindingLifetime`](#nimbuscontainerbindinglifetime)
- [`Nimbus\Container\Container`](#nimbuscontainercontainer)
- [`Nimbus\Container\ContainerState`](#nimbuscontainercontainerstate)
- [`Nimbus\ControllerHandler`](#nimbuscontrollerhandler)
- [`Nimbus\Database\ArcanoManager`](#nimbusdatabasearcanomanager)
- [`Nimbus\Database\ArcanoMigrationManager`](#nimbusdatabasearcanomigrationmanager)
- [`Nimbus\Database\ArcanoSchemaCompiler`](#nimbusdatabasearcanoschemacompiler)
- [`Nimbus\Database\ArcanoServiceProvider`](#nimbusdatabasearcanoserviceprovider)
- [`Nimbus\Database\ArcanoTenantMapper`](#nimbusdatabasearcanotenantmapper)
- [`Nimbus\Database\ArcanoTenantScope`](#nimbusdatabasearcanotenantscope)
- [`Nimbus\Database\DatabaseConfig`](#nimbusdatabasedatabaseconfig)
- [`Nimbus\Database\DatabaseHealthCheck`](#nimbusdatabasedatabasehealthcheck)
- [`Nimbus\Database\Factory`](#nimbusdatabasefactory)
- [`Nimbus\Database\Pgsql\AsyncPostgresDriver`](#nimbusdatabasepgsqlasyncpostgresdriver)
- [`Nimbus\Database\Pgsql\PgsqlException`](#nimbusdatabasepgsqlpgsqlexception)
- [`Nimbus\Database\QueryBudget`](#nimbusdatabasequerybudget)
- [`Nimbus\Database\QueryBudgetExceededException`](#nimbusdatabasequerybudgetexceededexception)
- [`Nimbus\Database\QueryBudgetViolation`](#nimbusdatabasequerybudgetviolation)
- [`Nimbus\Database\QueryMonitor`](#nimbusdatabasequerymonitor)
- [`Nimbus\Database\QueryReport`](#nimbusdatabasequeryreport)
- [`Nimbus\Database\Seeder`](#nimbusdatabaseseeder)
- [`Nimbus\Database\SeederRunner`](#nimbusdatabaseseederrunner)
- [`Nimbus\Events\EventDispatcher`](#nimbuseventseventdispatcher)
- [`Nimbus\Events\SyncEventDispatcher`](#nimbuseventssynceventdispatcher)
- [`Nimbus\HandlerResolver`](#nimbushandlerresolver)
- [`Nimbus\Health\HealthCheck`](#nimbushealthhealthcheck)
- [`Nimbus\Health\HealthRegistry`](#nimbushealthhealthregistry)
- [`Nimbus\Health\HealthResult`](#nimbushealthhealthresult)
- [`Nimbus\Health\HealthState`](#nimbushealthhealthstate)
- [`Nimbus\Http\ClientIpResolver`](#nimbushttpclientipresolver)
- [`Nimbus\Http\HttpException`](#nimbushttphttpexception)
- [`Nimbus\Http\JsonResource`](#nimbushttpjsonresource)
- [`Nimbus\Http\ProblemCode`](#nimbushttpproblemcode)
- [`Nimbus\Http\ResourceCollection`](#nimbushttpresourcecollection)
- [`Nimbus\Http\Responsable`](#nimbushttpresponsable)
- [`Nimbus\Http\ResponseSnapshot`](#nimbushttpresponsesnapshot)
- [`Nimbus\Idempotency\IdempotencyRecord`](#nimbusidempotencyidempotencyrecord)
- [`Nimbus\Idempotency\IdempotencyStore`](#nimbusidempotencyidempotencystore)
- [`Nimbus\Idempotency\MemoryIdempotencyStore`](#nimbusidempotencymemoryidempotencystore)
- [`Nimbus\Jobs\DatabaseJobQueue`](#nimbusjobsdatabasejobqueue)
- [`Nimbus\Jobs\ExponentialBackoff`](#nimbusjobsexponentialbackoff)
- [`Nimbus\Jobs\FailedJob`](#nimbusjobsfailedjob)
- [`Nimbus\Jobs\FailedJobRepository`](#nimbusjobsfailedjobrepository)
- [`Nimbus\Jobs\InvokableJobHandler`](#nimbusjobsinvokablejobhandler)
- [`Nimbus\Jobs\JobCodec`](#nimbusjobsjobcodec)
- [`Nimbus\Jobs\JobDispatcher`](#nimbusjobsjobdispatcher)
- [`Nimbus\Jobs\JobEnvelope`](#nimbusjobsjobenvelope)
- [`Nimbus\Jobs\JobHandler`](#nimbusjobsjobhandler)
- [`Nimbus\Jobs\JobOutcome`](#nimbusjobsjoboutcome)
- [`Nimbus\Jobs\JobQueue`](#nimbusjobsjobqueue)
- [`Nimbus\Jobs\JobRunResult`](#nimbusjobsjobrunresult)
- [`Nimbus\Jobs\JobState`](#nimbusjobsjobstate)
- [`Nimbus\Jobs\JobWorker`](#nimbusjobsjobworker)
- [`Nimbus\Jobs\MemoryJobDispatcher`](#nimbusjobsmemoryjobdispatcher)
- [`Nimbus\Jobs\QueueTablesMigration`](#nimbusjobsqueuetablesmigration)
- [`Nimbus\Jobs\RedisJobQueue`](#nimbusjobsredisjobqueue)
- [`Nimbus\Jobs\RetryPolicy`](#nimbusjobsretrypolicy)
- [`Nimbus\Jobs\SerializableJob`](#nimbusjobsserializablejob)
- [`Nimbus\Lifecycle\RequestLifecycleObserver`](#nimbuslifecyclerequestlifecycleobserver)
- [`Nimbus\Middleware\ArcanoLifecycleMiddleware`](#nimbusmiddlewarearcanolifecyclemiddleware)
- [`Nimbus\Middleware\AuthenticateMiddleware`](#nimbusmiddlewareauthenticatemiddleware)
- [`Nimbus\Middleware\AuthorizeMiddleware`](#nimbusmiddlewareauthorizemiddleware)
- [`Nimbus\Middleware\CorsMiddleware`](#nimbusmiddlewarecorsmiddleware)
- [`Nimbus\Middleware\DeadlineMiddleware`](#nimbusmiddlewaredeadlinemiddleware)
- [`Nimbus\Middleware\IdempotencyMiddleware`](#nimbusmiddlewareidempotencymiddleware)
- [`Nimbus\Middleware\ObserveRequestMiddleware`](#nimbusmiddlewareobserverequestmiddleware)
- [`Nimbus\Middleware\QueryBudgetMiddleware`](#nimbusmiddlewarequerybudgetmiddleware)
- [`Nimbus\Middleware\RateLimitMiddleware`](#nimbusmiddlewareratelimitmiddleware)
- [`Nimbus\Middleware\ResolveTenantMiddleware`](#nimbusmiddlewareresolvetenantmiddleware)
- [`Nimbus\Middleware\ResponseCacheMiddleware`](#nimbusmiddlewareresponsecachemiddleware)
- [`Nimbus\Middleware\SecurityHeadersMiddleware`](#nimbusmiddlewaresecurityheadersmiddleware)
- [`Nimbus\Middleware\TransactionalMiddleware`](#nimbusmiddlewaretransactionalmiddleware)
- [`Nimbus\Middleware\ValidateSignatureMiddleware`](#nimbusmiddlewarevalidatesignaturemiddleware)
- [`Nimbus\Observability\RequestObservation`](#nimbusobservabilityrequestobservation)
- [`Nimbus\Observability\RequestObserver`](#nimbusobservabilityrequestobserver)
- [`Nimbus\OpenApi\ClientGenerator`](#nimbusopenapiclientgenerator)
- [`Nimbus\OpenApi\ClientLanguage`](#nimbusopenapiclientlanguage)
- [`Nimbus\OpenApi\CompatibilityChange`](#nimbusopenapicompatibilitychange)
- [`Nimbus\OpenApi\CompatibilityChangeCode`](#nimbusopenapicompatibilitychangecode)
- [`Nimbus\OpenApi\CompatibilityChecker`](#nimbusopenapicompatibilitychecker)
- [`Nimbus\OpenApi\OpenApiGenerator`](#nimbusopenapiopenapigenerator)
- [`Nimbus\PackageDiscovery`](#nimbuspackagediscovery)
- [`Nimbus\Pagination\Page`](#nimbuspaginationpage)
- [`Nimbus\Pagination\Paginator`](#nimbuspaginationpaginator)
- [`Nimbus\PendingRoute`](#nimbuspendingroute)
- [`Nimbus\Pipeline`](#nimbuspipeline)
- [`Nimbus\Profiler\MemoryProfiler`](#nimbusprofilermemoryprofiler)
- [`Nimbus\Profiler\ProfilerMode`](#nimbusprofilerprofilermode)
- [`Nimbus\Profiler\RequestProfile`](#nimbusprofilerrequestprofile)
- [`Nimbus\RateLimit\MemoryRateLimitStore`](#nimbusratelimitmemoryratelimitstore)
- [`Nimbus\RateLimit\RateLimitDecision`](#nimbusratelimitratelimitdecision)
- [`Nimbus\RateLimit\RateLimitStore`](#nimbusratelimitratelimitstore)
- [`Nimbus\Redis\Connection`](#nimbusredisconnection)
- [`Nimbus\Redis\PamConnection`](#nimbusredispamconnection)
- [`Nimbus\Redis\RedisIdempotencyStore`](#nimbusredisredisidempotencystore)
- [`Nimbus\Redis\RedisPool`](#nimbusredisredispool)
- [`Nimbus\Redis\RedisRateLimitStore`](#nimbusredisredisratelimitstore)
- [`Nimbus\Redis\RedisResponseCacheStore`](#nimbusredisredisresponsecachestore)
- [`Nimbus\Resilience\CircuitBreaker`](#nimbusresiliencecircuitbreaker)
- [`Nimbus\Resilience\CircuitOpenException`](#nimbusresiliencecircuitopenexception)
- [`Nimbus\Resilience\CircuitState`](#nimbusresiliencecircuitstate)
- [`Nimbus\Resilience\RetryPolicy`](#nimbusresilienceretrypolicy)
- [`Nimbus\Route`](#nimbusroute)
- [`Nimbus\RouteConstraint`](#nimbusrouteconstraint)
- [`Nimbus\RouteRegistrar`](#nimbusrouteregistrar)
- [`Nimbus\Router`](#nimbusrouter)
- [`Nimbus\RoutingResult`](#nimbusroutingresult)
- [`Nimbus\RoutingResultType`](#nimbusroutingresulttype)
- [`Nimbus\Routing\RouteBindable`](#nimbusroutingroutebindable)
- [`Nimbus\Runtime\Background`](#nimbusruntimebackground)
- [`Nimbus\Runtime\Deadline`](#nimbusruntimedeadline)
- [`Nimbus\Scheduling\DatabaseSchedulerLock`](#nimbusschedulingdatabaseschedulerlock)
- [`Nimbus\Scheduling\MemorySchedulerLock`](#nimbusschedulingmemoryschedulerlock)
- [`Nimbus\Scheduling\RedisSchedulerLock`](#nimbusschedulingredisschedulerlock)
- [`Nimbus\Scheduling\Schedule`](#nimbusschedulingschedule)
- [`Nimbus\Scheduling\ScheduledTask`](#nimbusschedulingscheduledtask)
- [`Nimbus\Scheduling\Scheduler`](#nimbusschedulingscheduler)
- [`Nimbus\Scheduling\SchedulerLock`](#nimbusschedulingschedulerlock)
- [`Nimbus\Security\Crypt`](#nimbussecuritycrypt)
- [`Nimbus\Security\DecryptException`](#nimbussecuritydecryptexception)
- [`Nimbus\Security\Hash`](#nimbussecurityhash)
- [`Nimbus\Security\UrlSignatureState`](#nimbussecurityurlsignaturestate)
- [`Nimbus\Security\UrlSigner`](#nimbussecurityurlsigner)
- [`Nimbus\Storage\Disk`](#nimbusstoragedisk)
- [`Nimbus\Storage\S3UploadUrlGenerator`](#nimbusstorages3uploadurlgenerator)
- [`Nimbus\Storage\SignedStorageUrls`](#nimbusstoragesignedstorageurls)
- [`Nimbus\Storage\StorageConfig`](#nimbusstoragestorageconfig)
- [`Nimbus\Storage\StorageManager`](#nimbusstoragestoragemanager)
- [`Nimbus\Storage\StorageRoutes`](#nimbusstoragestorageroutes)
- [`Nimbus\Storage\StorageServiceProvider`](#nimbusstoragestorageserviceprovider)
- [`Nimbus\Storage\UploadUrlGenerator`](#nimbusstorageuploadurlgenerator)
- [`Nimbus\Tenancy\Tenant`](#nimbustenancytenant)
- [`Nimbus\Tenancy\TenantContext`](#nimbustenancytenantcontext)
- [`Nimbus\Tenancy\TenantResolver`](#nimbustenancytenantresolver)
- [`Nimbus\Testing\ActingAsAuthenticator`](#nimbustestingactingasauthenticator)
- [`Nimbus\Testing\TestCase`](#nimbustestingtestcase)
- [`Nimbus\Testing\TestClient`](#nimbustestingtestclient)
- [`Nimbus\Testing\TestResponse`](#nimbustestingtestresponse)
- [`Nimbus\Transactions\TransactionManager`](#nimbustransactionstransactionmanager)
- [`Nimbus\Validation\Attributes\Email`](#nimbusvalidationattributesemail)
- [`Nimbus\Validation\Attributes\Field`](#nimbusvalidationattributesfield)
- [`Nimbus\Validation\Attributes\In`](#nimbusvalidationattributesin)
- [`Nimbus\Validation\Attributes\Max`](#nimbusvalidationattributesmax)
- [`Nimbus\Validation\Attributes\Min`](#nimbusvalidationattributesmin)
- [`Nimbus\Validation\Attributes\Regex`](#nimbusvalidationattributesregex)
- [`Nimbus\Validation\Attributes\Url`](#nimbusvalidationattributesurl)
- [`Nimbus\Validation\Attributes\Uuid`](#nimbusvalidationattributesuuid)
- [`Nimbus\Validation\DtoHydrator`](#nimbusvalidationdtohydrator)
- [`Nimbus\Validation\EnumRule`](#nimbusvalidationenumrule)
- [`Nimbus\Validation\FormRequest`](#nimbusvalidationformrequest)
- [`Nimbus\Validation\Input`](#nimbusvalidationinput)
- [`Nimbus\Validation\Rule`](#nimbusvalidationrule)
- [`Nimbus\Validation\ValidationException`](#nimbusvalidationvalidationexception)
- [`Nimbus\Validation\ValidationRule`](#nimbusvalidationvalidationrule)

## `Nimbus\App`

```php
final class Nimbus\App implements Pam\Contracts\Http\ApplicationInterface
```

### Methods

- `public __construct(bool $discoverPackages = true, ?Nimbus\Container\Container $container = null): mixed`
- `public background(): Nimbus\Runtime\Background`
- `public boot(): self`
- `public container(): Nimbus\Container\Container`
- `public controller(string $controller, callable $routes): void`
- `public delete(string $path, array|callable|string $handler): Nimbus\PendingRoute`
- `public docs(string $path = "/docs", string $title = "Nimbus API", string $version = "1.0.0"): self`
- `public get(string $path, array|callable|string $handler): Nimbus\PendingRoute`
- `public group(callable $routes): void`
- `public handle(Pam\Http\Request $request, Pam\Http\Response $response): Pam\Http\Response`
- `public handler(callable|object $handler): self`
- `public head(string $path, array|callable|string $handler): Nimbus\PendingRoute`
- `public listen(int $port, string $host = "127.0.0.1", array $options = []): void`
- `public middleware(callable|object|string $middleware): self`
- `public observe(Nimbus\Lifecycle\RequestLifecycleObserver $observer): self`
- `public onError(callable $handler): self`
- `public openApi(string $title = "Nimbus API", string $version = "1.0.0"): Nimbus\OpenApi\OpenApiGenerator`
- `public options(string $path, array|callable|string $handler): Nimbus\PendingRoute`
- `public patch(string $path, array|callable|string $handler): Nimbus\PendingRoute`
- `public post(string $path, array|callable|string $handler): Nimbus\PendingRoute`
- `public prefix(string $prefix): Nimbus\RouteRegistrar`
- `public provider(Pam\Contracts\Package\ServiceProviderInterface $provider): self`
- `public put(string $path, array|callable|string $handler): Nimbus\PendingRoute`
- `public resource(string $path, string $controller, ?callable $routes = null): void`
- `public route(string $method, string $path, array|callable|string $handler): self`
- `public routes(string $file): self`
- `public storage(): Nimbus\Storage\StorageManager`
- `public transport(Pam\Contracts\Transport\TransportProviderInterface $provider): self`
- `public transports(): array`

## `Nimbus\Auth\AuthContext`

```php
final readonly class Nimbus\Auth\AuthContext
```

### Properties

- `public readonly Nimbus\Auth\Principal $principal`

### Methods

- `public __construct(Nimbus\Auth\Principal $principal): mixed`

## `Nimbus\Auth\Authenticator`

```php
abstract interface Nimbus\Auth\Authenticator
```

### Methods

- `public authenticate(Pam\Http\Request $request): ?Nimbus\Auth\Principal`

## `Nimbus\Auth\BearerTokenAuthenticator`

```php
final readonly class Nimbus\Auth\BearerTokenAuthenticator implements Nimbus\Auth\Authenticator
```

### Methods

- `public __construct(Nimbus\Auth\HmacTokenCodec $tokens, ?Nimbus\Auth\TokenRevocationStore $revocations = null, ?callable $clock = null): mixed`
- `public authenticate(Pam\Http\Request $request): ?Nimbus\Auth\Principal`

## `Nimbus\Auth\HmacTokenCodec`

```php
final readonly class Nimbus\Auth\HmacTokenCodec
```

### Methods

- `public __construct(string $secret, string $issuer, string $audience, int $leewaySeconds = 5, ?callable $clock = null, string $keyIdentifier = "primary", array $verificationKeys = []): mixed`
- `public issue(string $subject, array $abilities, int $ttlSeconds = 900): string`
- `public verify(string $token): ?Nimbus\Auth\TokenPrincipal`

## `Nimbus\Auth\MemoryTokenRevocationStore`

```php
final class Nimbus\Auth\MemoryTokenRevocationStore implements Nimbus\Auth\TokenRevocationStore
```

### Methods

- `public __construct(int $maximumEntries = 10000): mixed`
- `public isRevoked(string $tokenIdentifier, int $now): bool`
- `public revoke(string $tokenIdentifier, int $expiresAt): void`

## `Nimbus\Auth\Principal`

```php
abstract interface Nimbus\Auth\Principal
```

### Methods

- `public can(string $ability): bool`
- `public identifier(): string`

## `Nimbus\Auth\TokenPrincipal`

```php
final readonly class Nimbus\Auth\TokenPrincipal implements Nimbus\Auth\Principal
```

### Properties

- `public readonly array $abilities`
- `public readonly string $tokenIdentifier`

### Methods

- `public __construct(string $subject, array $abilities, string $tokenIdentifier): mixed`
- `public can(string $ability): bool`
- `public identifier(): string`

## `Nimbus\Auth\TokenRevocationStore`

```php
abstract interface Nimbus\Auth\TokenRevocationStore
```

### Methods

- `public isRevoked(string $tokenIdentifier, int $now): bool`
- `public revoke(string $tokenIdentifier, int $expiresAt): void`

## `Nimbus\Cache\CacheRecord`

```php
final readonly class Nimbus\Cache\CacheRecord
```

### Properties

- `public readonly int $expiresAt`
- `public readonly Nimbus\Http\ResponseSnapshot $response`

### Methods

- `public __construct(Nimbus\Http\ResponseSnapshot $response, int $expiresAt): mixed`

## `Nimbus\Cache\MemoryResponseCacheStore`

```php
final class Nimbus\Cache\MemoryResponseCacheStore implements Nimbus\Cache\ResponseCacheStore
```

### Methods

- `public __construct(int $maximumRecords = 10000): mixed`
- `public forget(string $key): void`
- `public get(string $key, int $now): ?Nimbus\Cache\CacheRecord`
- `public put(string $key, Nimbus\Cache\CacheRecord $record): void`

## `Nimbus\Cache\ResponseCacheStore`

```php
abstract interface Nimbus\Cache\ResponseCacheStore
```

### Methods

- `public forget(string $key): void`
- `public get(string $key, int $now): ?Nimbus\Cache\CacheRecord`
- `public put(string $key, Nimbus\Cache\CacheRecord $record): void`

## `Nimbus\CallableRequestHandler`

```php
final readonly class Nimbus\CallableRequestHandler implements Pam\Contracts\Http\RequestHandlerInterface
```

### Methods

- `public __construct(callable $handler): mixed`
- `public handle(Pam\Http\Request $request, Pam\Http\Response $response): Pam\Http\Response`

## `Nimbus\Config\ConfigDefinition`

```php
final readonly class Nimbus\Config\ConfigDefinition
```

### Properties

- `public readonly bool|float|int|null|string $default`
- `public readonly string $environment`
- `public readonly string $key`
- `public readonly bool $required`
- `public readonly bool $sensitive`
- `public readonly Nimbus\Config\ConfigType $type`

### Methods

- `public __construct(string $key, string $environment, Nimbus\Config\ConfigType $type = 1, bool $required = true, bool|float|int|null|string $default = null, bool $sensitive = false): mixed`

## `Nimbus\Config\ConfigType`

```php
final enum Nimbus\Config\ConfigType implements BackedEnum, UnitEnum: int
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

## `Nimbus\Config\Configuration`

```php
final readonly class Nimbus\Config\Configuration
```

### Methods

- `public boolean(string $key): bool`
- `public float(string $key): float`
- `public static fromArray(array $definitions, array $environment): self`
- `public static fromEnvironment(array $definitions): self`
- `public integer(string $key): int`
- `public redacted(): array`
- `public string(string $key): string`

## `Nimbus\Config\ConfigurationException`

```php
final class Nimbus\Config\ConfigurationException extends RuntimeException implements Stringable, Throwable
```

### Properties

- `public readonly array $errors`

### Methods

- `public __construct(array $errors): mixed`

## `Nimbus\ContainerMiddleware`

```php
final readonly class Nimbus\ContainerMiddleware implements Pam\Contracts\Http\MiddlewareInterface
```

### Methods

- `public __construct(Nimbus\Container\Container $container, string $middleware): mixed`
- `public is(string $middleware): bool`
- `public process(Pam\Http\Request $request, Pam\Http\Response $response, Pam\Contracts\Http\RequestHandlerInterface $next): Pam\Http\Response`

## `Nimbus\Container\Binding`

```php
final readonly class Nimbus\Container\Binding
```

### Properties

- `public readonly Closure $factory`
- `public readonly Nimbus\Container\BindingLifetime $lifetime`

### Methods

- `public __construct(callable $factory, Nimbus\Container\BindingLifetime $lifetime): mixed`

## `Nimbus\Container\BindingLifetime`

```php
final enum Nimbus\Container\BindingLifetime implements BackedEnum, UnitEnum: int
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

## `Nimbus\Container\Container`

```php
final class Nimbus\Container\Container
```

### Methods

- `public __construct(): mixed`
- `public beginScope(): void`
- `public bind(string $id, callable|null|string $factory = null): self`
- `public bindRoute(string $class, callable $resolver): self`
- `public call(array $callable, array $named = [], array $provided = []): mixed`
- `public callFunction(Closure $function, array $named = [], array $provided = []): mixed`
- `public diagnostics(): array`
- `public endScope(): void`
- `public get(string $id): mixed`
- `public has(string $id): bool`
- `public instance(string $id, mixed $instance): self`
- `public scoped(string $id, callable|null|string $factory = null): self`
- `public scopedInstance(string $id, mixed $instance): self`
- `public scopedValue(string $id): mixed`
- `public singleton(string $id, callable|null|string $factory = null): self`

## `Nimbus\Container\ContainerState`

```php
final enum Nimbus\Container\ContainerState implements BackedEnum, UnitEnum: int
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

## `Nimbus\ControllerHandler`

```php
final readonly class Nimbus\ControllerHandler
```

### Methods

- `public __construct(Nimbus\Container\Container $container, string $controller, string $method): mixed`
- `public __invoke(Pam\Http\Request $request, Pam\Http\Response $response): mixed`

## `Nimbus\Database\ArcanoManager`

```php
final class Nimbus\Database\ArcanoManager implements Nimbus\Transactions\TransactionManager
```

### Methods

- `public __construct(Nimbus\Database\ArcanoDatabaseProvider $databases, Arcano\ORM\SchemaInterface|array $schema, ?Nimbus\Container\Container $container = null): mixed`
- `public database(UnitEnum|null|string $name = null): Arcano\Database\DatabaseInterface`
- `public entityManager(): Arcano\ORM\EntityManagerInterface`
- `public orm(): Arcano\ORM\ORMInterface`
- `public protectTenant(string $role, string $column = "tenant_id"): void`
- `public releaseCurrentRequest(): void`
- `public repository(string $role): Arcano\ORM\RepositoryInterface`
- `public select(string $role): Arcano\ORM\Select`
- `public transaction(callable $operation): mixed`

## `Nimbus\Database\ArcanoMigrationManager`

```php
final class Nimbus\Database\ArcanoMigrationManager
```

### Methods

- `public __construct(Nimbus\Database\ArcanoDatabaseProvider $databases, string $directory, string $table = "migrations", string $namespace = "Migration", ?Nimbus\Database\ArcanoSchemaCompiler $entities = null): mixed`
- `public generate(): array`
- `public migrate(): array`
- `public publish(string $name, string $class): ?string`
- `public rollback(int $steps = 1): array`
- `public status(): array`

## `Nimbus\Database\ArcanoSchemaCompiler`

```php
final readonly class Nimbus\Database\ArcanoSchemaCompiler
```

### Methods

- `public __construct(array|string $directories, ?string $cacheFile = null, bool $refresh = true): mixed`
- `public clear(): void`
- `public compile(Arcano\Database\DatabaseProviderInterface $databases, array $generators = []): array`
- `public schema(Arcano\Database\DatabaseProviderInterface $databases): array`

## `Nimbus\Database\ArcanoServiceProvider`

```php
final readonly class Nimbus\Database\ArcanoServiceProvider implements Pam\Contracts\Package\ServiceProviderInterface
```

### Methods

- `public __construct(Nimbus\Database\DatabaseConfig $config, Arcano\ORM\SchemaInterface|Nimbus\Database\ArcanoSchemaCompiler|array $schema, ?string $migrations = null, int $maxIdleConnections = 8): mixed`
- `public boot(Pam\Contracts\Http\ApplicationInterface $application): void`
- `public register(Pam\Contracts\Http\ApplicationInterface $application): void`

## `Nimbus\Database\ArcanoTenantMapper`

```php
class Nimbus\Database\ArcanoTenantMapper extends Arcano\ORM\Mapper\Mapper implements Arcano\ORM\MapperInterface
```

### Methods

- `public queueCreate(object $entity, Arcano\ORM\Heap\Node $node, Arcano\ORM\Heap\State $state): Arcano\ORM\Command\CommandInterface`

## `Nimbus\Database\ArcanoTenantScope`

```php
final readonly class Nimbus\Database\ArcanoTenantScope implements Arcano\ORM\Select\ScopeInterface
```

### Methods

- `public __construct(Nimbus\Container\Container $container, string $column = "tenant_id"): mixed`
- `public apply(Arcano\ORM\Select\QueryBuilder $query): void`
- `public assign(Arcano\ORM\Heap\State $state): void`

## `Nimbus\Database\DatabaseConfig`

```php
final readonly class Nimbus\Database\DatabaseConfig
```

### Properties

- `public readonly array $connections`
- `public readonly string $defaultConnection`

### Methods

- `public __construct(string $defaultConnection, array $connections): mixed`
- `public static fromEnvironment(): self`

## `Nimbus\Database\DatabaseHealthCheck`

```php
final readonly class Nimbus\Database\DatabaseHealthCheck implements Nimbus\Health\HealthCheck
```

### Methods

- `public __construct(Nimbus\Database\ArcanoManager $arcano, ?string $connection = null): mixed`
- `public check(): Nimbus\Health\HealthResult`

## `Nimbus\Database\Factory`

```php
abstract class Nimbus\Database\Factory
```

### Methods

- `public __construct(): mixed`
- `public afterCreating(Closure $callback): static`
- `public afterMaking(Closure $callback): static`
- `public count(int $count): static`
- `public create(array $attributes = []): array|object`
- `public createMany(array $attributes = []): array`
- `public createOne(array $attributes = []): object`
- `public definition(): array`
- `public make(array $attributes = []): array|object`
- `public makeMany(array $attributes = []): array`
- `public makeOne(array $attributes = []): object`
- `public static new(): static`
- `public raw(array $attributes = []): array`
- `public sequence(Closure|array ...$states): static`
- `public state(Closure|array $state): static`
- `public static useFaker(?Faker\Generator $faker): void`
- `public static useManager(?Nimbus\Database\ArcanoManager $manager): ?Nimbus\Database\ArcanoManager`

## `Nimbus\Database\Pgsql\AsyncPostgresDriver`

```php
class Nimbus\Database\Pgsql\AsyncPostgresDriver extends Arcano\Database\Driver\Postgres\PostgresDriver implements Arcano\Database\Driver\CursorInterface, Arcano\Database\Driver\DriverInterface, Arcano\Database\NamedInterface, Psr\Log\LoggerAwareInterface
```

### Methods

- `public static available(): bool`
- `public static conninfo(string $dsn, ?string $username, ?string $password): string`
- `public healthy(): bool`

## `Nimbus\Database\Pgsql\PgsqlException`

```php
final class Nimbus\Database\Pgsql\PgsqlException extends PDOException implements Stringable, Throwable
```

### Methods

- `public static from(string $sqlState, string $message): self`

## `Nimbus\Database\QueryBudget`

```php
final readonly class Nimbus\Database\QueryBudget
```

### Properties

- `public readonly int $maximumDuplicateQueries`
- `public readonly float $maximumElapsedMilliseconds`
- `public readonly int $maximumQueries`

### Methods

- `public __construct(int $maximumQueries = 100, float $maximumElapsedMilliseconds = 500, int $maximumDuplicateQueries = 10): mixed`

## `Nimbus\Database\QueryBudgetExceededException`

```php
final class Nimbus\Database\QueryBudgetExceededException extends RuntimeException implements Stringable, Throwable
```

### Properties

- `public readonly Nimbus\Database\QueryReport $report`

### Methods

- `public __construct(Nimbus\Database\QueryReport $report): mixed`

## `Nimbus\Database\QueryBudgetViolation`

```php
final enum Nimbus\Database\QueryBudgetViolation implements BackedEnum, UnitEnum: int
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

## `Nimbus\Database\QueryMonitor`

```php
final class Nimbus\Database\QueryMonitor
```

### Methods

- `public __construct(): mixed`
- `public begin(Nimbus\Database\QueryBudget $budget): void`
- `public finish(): Nimbus\Database\QueryReport`
- `public static forArcano(Nimbus\Database\ArcanoDatabaseProvider $databases): self`

## `Nimbus\Database\QueryReport`

```php
final readonly class Nimbus\Database\QueryReport
```

### Properties

- `public readonly int $count`
- `public readonly array $duplicates`
- `public readonly float $elapsedMilliseconds`
- `public readonly array $violations`

### Methods

- `public __construct(int $count, float $elapsedMilliseconds, array $duplicates, array $violations): mixed`
- `public withinBudget(): bool`

## `Nimbus\Database\Seeder`

```php
abstract class Nimbus\Database\Seeder
```

### Methods

- `public call(array|string $seeders): void`
- `public invoke(?Nimbus\Container\Container $container = null): array`

## `Nimbus\Database\SeederRunner`

```php
final readonly class Nimbus\Database\SeederRunner
```

### Methods

- `public __construct(Nimbus\App $app): mixed`
- `public run(string $seeder): array`

## `Nimbus\Events\EventDispatcher`

```php
abstract interface Nimbus\Events\EventDispatcher
```

### Methods

- `public dispatch(object $event): object`

## `Nimbus\Events\SyncEventDispatcher`

```php
final class Nimbus\Events\SyncEventDispatcher implements Nimbus\Events\EventDispatcher
```

### Methods

- `public __construct(Nimbus\Container\Container $container): mixed`
- `public dispatch(object $event): object`
- `public listen(string $event, callable|string $listener): self`

## `Nimbus\HandlerResolver`

```php
final readonly class Nimbus\HandlerResolver
```

### Methods

- `public __construct(Nimbus\Container\Container $container): mixed`
- `public resolve(array|callable|string $handler): Closure`

## `Nimbus\Health\HealthCheck`

```php
abstract interface Nimbus\Health\HealthCheck
```

### Methods

- `public check(): Nimbus\Health\HealthResult`

## `Nimbus\Health\HealthRegistry`

```php
final class Nimbus\Health\HealthRegistry
```

### Methods

- `public add(string $name, Nimbus\Health\HealthCheck $check): self`
- `public run(): array`

## `Nimbus\Health\HealthResult`

```php
final readonly class Nimbus\Health\HealthResult
```

### Properties

- `public readonly array $details`
- `public readonly Nimbus\Health\HealthState $state`

### Methods

- `public __construct(Nimbus\Health\HealthState $state, array $details = []): mixed`

## `Nimbus\Health\HealthState`

```php
final enum Nimbus\Health\HealthState implements BackedEnum, UnitEnum: int
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

## `Nimbus\Http\ClientIpResolver`

```php
final readonly class Nimbus\Http\ClientIpResolver
```

### Methods

- `public __construct(array $trustedProxies = []): mixed`
- `public resolve(Pam\Http\Request $request): string`

## `Nimbus\Http\HttpException`

```php
class Nimbus\Http\HttpException extends RuntimeException implements Stringable, Throwable
```

### Properties

- `public readonly array $details`
- `public readonly Nimbus\Http\ProblemCode $problemCode`
- `public readonly int $status`

### Methods

- `public __construct(int $status, Nimbus\Http\ProblemCode $problemCode, string $message, array $details = [], ?Throwable $previous = null): mixed`

## `Nimbus\Http\JsonResource`

```php
abstract readonly class Nimbus\Http\JsonResource implements Nimbus\Http\Responsable
```

### Methods

- `public __construct(mixed $resource, int $status = 200): mixed`
- `public static collection(Nimbus\Pagination\Page|Traversable|array $resources): Nimbus\Http\ResourceCollection`
- `public toArray(Pam\Http\Request $request): array`
- `public toResponse(Pam\Http\Request $request, Pam\Http\Response $response): Pam\Http\Response`

## `Nimbus\Http\ProblemCode`

```php
final enum Nimbus\Http\ProblemCode implements BackedEnum, UnitEnum: int
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
- `InvalidSignature = 10`
- `PayloadTooLarge = 11`
- `UnsupportedMediaType = 12`

### Constants

- `Conflict = 5`
- `Forbidden = 3`
- `InternalError = 8`
- `InvalidSignature = 10`
- `MethodNotAllowed = 9`
- `NotFound = 4`
- `PayloadTooLarge = 11`
- `RateLimited = 6`
- `Timeout = 7`
- `Unauthenticated = 2`
- `UnsupportedMediaType = 12`
- `ValidationFailed = 1`

### Properties

- `public readonly string $name`
- `public readonly int $value`

### Methods

- `public static cases(): array`
- `public static from(int|string $value): static`
- `public static tryFrom(int|string $value): ?static`

## `Nimbus\Http\ResourceCollection`

```php
final readonly class Nimbus\Http\ResourceCollection implements Nimbus\Http\Responsable
```

### Methods

- `public __construct(array $resources, array $meta = [], array $links = []): mixed`
- `public toResponse(Pam\Http\Request $request, Pam\Http\Response $response): Pam\Http\Response`
- `public withMeta(array $meta): self`

## `Nimbus\Http\Responsable`

```php
abstract interface Nimbus\Http\Responsable
```

### Methods

- `public toResponse(Pam\Http\Request $request, Pam\Http\Response $response): Pam\Http\Response`

## `Nimbus\Http\ResponseSnapshot`

```php
final readonly class Nimbus\Http\ResponseSnapshot
```

### Properties

- `public readonly string $body`
- `public readonly array $headers`
- `public readonly int $status`

### Methods

- `public __construct(int $status, array $headers, string $body): mixed`
- `public static capture(Pam\Http\Response $response): self`
- `public restore(Pam\Http\Response $response): Pam\Http\Response`

## `Nimbus\Idempotency\IdempotencyRecord`

```php
final readonly class Nimbus\Idempotency\IdempotencyRecord
```

### Properties

- `public readonly int $expiresAt`
- `public readonly string $fingerprint`
- `public readonly Nimbus\Http\ResponseSnapshot $response`

### Methods

- `public __construct(string $fingerprint, Nimbus\Http\ResponseSnapshot $response, int $expiresAt): mixed`

## `Nimbus\Idempotency\IdempotencyStore`

```php
abstract interface Nimbus\Idempotency\IdempotencyStore
```

### Methods

- `public get(string $key, int $now): ?Nimbus\Idempotency\IdempotencyRecord`
- `public put(string $key, Nimbus\Idempotency\IdempotencyRecord $record): void`

## `Nimbus\Idempotency\MemoryIdempotencyStore`

```php
final class Nimbus\Idempotency\MemoryIdempotencyStore implements Nimbus\Idempotency\IdempotencyStore
```

### Methods

- `public __construct(int $maximumRecords = 10000): mixed`
- `public get(string $key, int $now): ?Nimbus\Idempotency\IdempotencyRecord`
- `public put(string $key, Nimbus\Idempotency\IdempotencyRecord $record): void`

## `Nimbus\Jobs\DatabaseJobQueue`

```php
final class Nimbus\Jobs\DatabaseJobQueue implements Nimbus\Jobs\FailedJobRepository, Nimbus\Jobs\JobDispatcher, Nimbus\Jobs\JobQueue
```

### Methods

- `public __construct(Arcano\Database\DatabaseInterface|Nimbus\Database\ArcanoDatabaseProvider $database, Nimbus\Jobs\JobCodec $codec, string $queue = "default", ?string $connection = null, ?Closure $clock = null): mixed`
- `public complete(Nimbus\Jobs\JobEnvelope $envelope): void`
- `public deadLetter(Nimbus\Jobs\JobEnvelope $envelope, ?Throwable $failure = null): void`
- `public dispatch(object $job, int $maximumAttempts = 3, int $delaySeconds = 0): Nimbus\Jobs\JobEnvelope`
- `public failed(int $limit = 100): array`
- `public forgetFailed(string $id): bool`
- `public release(Nimbus\Jobs\JobEnvelope $envelope, int $availableAt, ?Throwable $failure = null): void`
- `public reserve(int $now, int $leaseSeconds): ?Nimbus\Jobs\JobEnvelope`
- `public retryFailed(string $id): bool`
- `public size(): int`

## `Nimbus\Jobs\ExponentialBackoff`

```php
final readonly class Nimbus\Jobs\ExponentialBackoff implements Nimbus\Jobs\RetryPolicy
```

### Methods

- `public __construct(int $baseSeconds = 5, int $maximumSeconds = 3600, bool $jitter = true, ?Closure $random = null): mixed`
- `public delaySeconds(int $attempt): int`

## `Nimbus\Jobs\FailedJob`

```php
final readonly class Nimbus\Jobs\FailedJob
```

### Properties

- `public readonly int $attempts`
- `public readonly int $failedAt`
- `public readonly ?string $failureClass`
- `public readonly string $id`
- `public readonly string $payload`
- `public readonly string $queue`

### Methods

- `public __construct(string $id, string $queue, string $payload, int $attempts, ?string $failureClass, int $failedAt): mixed`

## `Nimbus\Jobs\FailedJobRepository`

```php
abstract interface Nimbus\Jobs\FailedJobRepository
```

### Methods

- `public failed(int $limit = 100): array`
- `public forgetFailed(string $id): bool`
- `public retryFailed(string $id): bool`

## `Nimbus\Jobs\InvokableJobHandler`

```php
final readonly class Nimbus\Jobs\InvokableJobHandler implements Nimbus\Jobs\JobHandler
```

### Methods

- `public __construct(?Nimbus\Container\Container $container = null): mixed`
- `public handle(object $job): Nimbus\Jobs\JobOutcome`

## `Nimbus\Jobs\JobCodec`

```php
final readonly class Nimbus\Jobs\JobCodec
```

### Methods

- `public __construct(array $jobs): mixed`
- `public decode(string $json): Nimbus\Jobs\SerializableJob`
- `public encode(Nimbus\Jobs\SerializableJob $job): string`

## `Nimbus\Jobs\JobDispatcher`

```php
abstract interface Nimbus\Jobs\JobDispatcher
```

### Methods

- `public dispatch(object $job, int $maximumAttempts = 3, int $delaySeconds = 0): Nimbus\Jobs\JobEnvelope`

## `Nimbus\Jobs\JobEnvelope`

```php
final class Nimbus\Jobs\JobEnvelope
```

### Properties

- `public int $attempts`
- `public int $availableAt`
- `public readonly string $id`
- `public readonly object $job`
- `public ?string $lastFailureClass`
- `public ?int $leaseUntil`
- `public readonly int $maximumAttempts`
- `public Nimbus\Jobs\JobState $state`

### Methods

- `public __construct(string $id, object $job, int $maximumAttempts = 3, int $availableAt = 0): mixed`

## `Nimbus\Jobs\JobHandler`

```php
abstract interface Nimbus\Jobs\JobHandler
```

### Methods

- `public handle(object $job): Nimbus\Jobs\JobOutcome`

## `Nimbus\Jobs\JobOutcome`

```php
final enum Nimbus\Jobs\JobOutcome implements BackedEnum, UnitEnum: int
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

## `Nimbus\Jobs\JobQueue`

```php
abstract interface Nimbus\Jobs\JobQueue implements Nimbus\Jobs\JobDispatcher
```

### Methods

- `public complete(Nimbus\Jobs\JobEnvelope $envelope): void`
- `public deadLetter(Nimbus\Jobs\JobEnvelope $envelope, ?Throwable $failure = null): void`
- `public release(Nimbus\Jobs\JobEnvelope $envelope, int $availableAt, ?Throwable $failure = null): void`
- `public reserve(int $now, int $leaseSeconds): ?Nimbus\Jobs\JobEnvelope`

## `Nimbus\Jobs\JobRunResult`

```php
final enum Nimbus\Jobs\JobRunResult implements BackedEnum, UnitEnum: int
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

## `Nimbus\Jobs\JobState`

```php
final enum Nimbus\Jobs\JobState implements BackedEnum, UnitEnum: int
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

## `Nimbus\Jobs\JobWorker`

```php
final readonly class Nimbus\Jobs\JobWorker
```

### Methods

- `public __construct(Nimbus\Jobs\JobQueue $queue, Nimbus\Jobs\JobHandler|callable $handler, int $leaseSeconds = 30, int $retryDelaySeconds = 5, ?Nimbus\Jobs\RetryPolicy $retryPolicy = null): mixed`
- `public runOne(?int $now = null): Nimbus\Jobs\JobRunResult`
- `public work(int $maximumJobs = 100, float $maximumSeconds = 1, ?Closure $clock = null): int`

## `Nimbus\Jobs\MemoryJobDispatcher`

```php
final class Nimbus\Jobs\MemoryJobDispatcher implements Nimbus\Jobs\JobDispatcher, Nimbus\Jobs\JobQueue
```

### Methods

- `public __construct(int $maximumQueuedJobs = 10000): mixed`
- `public complete(Nimbus\Jobs\JobEnvelope $envelope): void`
- `public deadLetter(Nimbus\Jobs\JobEnvelope $envelope, ?Throwable $failure = null): void`
- `public deadLetters(): array`
- `public dispatch(object $job, int $maximumAttempts = 3, int $delaySeconds = 0): Nimbus\Jobs\JobEnvelope`
- `public pending(): array`
- `public release(Nimbus\Jobs\JobEnvelope $envelope, int $availableAt, ?Throwable $failure = null): void`
- `public reserve(int $now, int $leaseSeconds): ?Nimbus\Jobs\JobEnvelope`

## `Nimbus\Jobs\QueueTablesMigration`

```php
final class Nimbus\Jobs\QueueTablesMigration extends Arcano\Migrations\Migration implements Arcano\Migrations\MigrationInterface
```

### Methods

- `public down(): void`
- `public up(): void`

## `Nimbus\Jobs\RedisJobQueue`

```php
final class Nimbus\Jobs\RedisJobQueue implements Nimbus\Jobs\FailedJobRepository, Nimbus\Jobs\JobDispatcher, Nimbus\Jobs\JobQueue
```

### Methods

- `public __construct(Nimbus\Redis\Connection $redis, Nimbus\Jobs\JobCodec $codec, string $queue = "default", string $prefix = "nimbus:queue:", ?Closure $clock = null): mixed`
- `public complete(Nimbus\Jobs\JobEnvelope $envelope): void`
- `public deadLetter(Nimbus\Jobs\JobEnvelope $envelope, ?Throwable $failure = null): void`
- `public dispatch(object $job, int $maximumAttempts = 3, int $delaySeconds = 0): Nimbus\Jobs\JobEnvelope`
- `public failed(int $limit = 100): array`
- `public forgetFailed(string $id): bool`
- `public release(Nimbus\Jobs\JobEnvelope $envelope, int $availableAt, ?Throwable $failure = null): void`
- `public reserve(int $now, int $leaseSeconds): ?Nimbus\Jobs\JobEnvelope`
- `public retryFailed(string $id): bool`
- `public size(): int`

## `Nimbus\Jobs\RetryPolicy`

```php
abstract interface Nimbus\Jobs\RetryPolicy
```

### Methods

- `public delaySeconds(int $attempt): int`

## `Nimbus\Jobs\SerializableJob`

```php
abstract interface Nimbus\Jobs\SerializableJob
```

### Methods

- `public static fromJobPayload(array $payload): self`
- `public static jobName(): string`
- `public toJobPayload(): array`

## `Nimbus\Lifecycle\RequestLifecycleObserver`

```php
abstract interface Nimbus\Lifecycle\RequestLifecycleObserver
```

### Methods

- `public finished(Pam\Http\Request $request, Pam\Http\Response $response, ?Throwable $failure): void`
- `public starting(Pam\Http\Request $request): void`

## `Nimbus\Middleware\ArcanoLifecycleMiddleware`

```php
final readonly class Nimbus\Middleware\ArcanoLifecycleMiddleware implements Pam\Contracts\Http\MiddlewareInterface
```

### Methods

- `public __construct(Nimbus\Database\ArcanoManager $arcano): mixed`
- `public process(Pam\Http\Request $request, Pam\Http\Response $response, Pam\Contracts\Http\RequestHandlerInterface $next): Pam\Http\Response`

## `Nimbus\Middleware\AuthenticateMiddleware`

```php
final readonly class Nimbus\Middleware\AuthenticateMiddleware implements Pam\Contracts\Http\MiddlewareInterface
```

### Methods

- `public __construct(Nimbus\Auth\Authenticator $authenticator, Nimbus\Container\Container $container): mixed`
- `public process(Pam\Http\Request $request, Pam\Http\Response $response, Pam\Contracts\Http\RequestHandlerInterface $next): Pam\Http\Response`

## `Nimbus\Middleware\AuthorizeMiddleware`

```php
final readonly class Nimbus\Middleware\AuthorizeMiddleware implements Pam\Contracts\Http\MiddlewareInterface
```

### Methods

- `public __construct(Nimbus\Container\Container $container, string $ability): mixed`
- `public process(Pam\Http\Request $request, Pam\Http\Response $response, Pam\Contracts\Http\RequestHandlerInterface $next): Pam\Http\Response`

## `Nimbus\Middleware\CorsMiddleware`

```php
final readonly class Nimbus\Middleware\CorsMiddleware implements Pam\Contracts\Http\MiddlewareInterface
```

### Methods

- `public __construct(array $origins, array $methods = ["GET","POST","PUT","PATCH","DELETE","OPTIONS"], array $headers = ["content-type","authorization"], bool $credentials = false, int $maxAge = 600): mixed`
- `public process(Pam\Http\Request $request, Pam\Http\Response $response, Pam\Contracts\Http\RequestHandlerInterface $next): Pam\Http\Response`

## `Nimbus\Middleware\DeadlineMiddleware`

```php
final readonly class Nimbus\Middleware\DeadlineMiddleware implements Pam\Contracts\Http\MiddlewareInterface
```

### Methods

- `public __construct(Nimbus\Container\Container $container, float $seconds): mixed`
- `public process(Pam\Http\Request $request, Pam\Http\Response $response, Pam\Contracts\Http\RequestHandlerInterface $next): Pam\Http\Response`

## `Nimbus\Middleware\IdempotencyMiddleware`

```php
final readonly class Nimbus\Middleware\IdempotencyMiddleware implements Pam\Contracts\Http\MiddlewareInterface
```

### Methods

- `public __construct(Nimbus\Idempotency\IdempotencyStore $store, int $ttlSeconds = 86400, bool $required = true): mixed`
- `public process(Pam\Http\Request $request, Pam\Http\Response $response, Pam\Contracts\Http\RequestHandlerInterface $next): Pam\Http\Response`

## `Nimbus\Middleware\ObserveRequestMiddleware`

```php
final readonly class Nimbus\Middleware\ObserveRequestMiddleware implements Pam\Contracts\Http\MiddlewareInterface
```

### Methods

- `public __construct(Nimbus\Observability\RequestObserver $observer, ?Nimbus\Container\Container $container = null): mixed`
- `public process(Pam\Http\Request $request, Pam\Http\Response $response, Pam\Contracts\Http\RequestHandlerInterface $next): Pam\Http\Response`

## `Nimbus\Middleware\QueryBudgetMiddleware`

```php
final readonly class Nimbus\Middleware\QueryBudgetMiddleware implements Pam\Contracts\Http\MiddlewareInterface
```

### Methods

- `public __construct(Nimbus\Database\QueryMonitor $monitor, Nimbus\Database\QueryBudget $budget = {"@object":"Nimbus\\Database\\QueryBudget","properties":{"maximumDuplicateQueries":10,"maximumElapsedMilliseconds":500,"maximumQueries":100}}, bool $failOnViolation = false): mixed`
- `public process(Pam\Http\Request $request, Pam\Http\Response $response, Pam\Contracts\Http\RequestHandlerInterface $next): Pam\Http\Response`

## `Nimbus\Middleware\RateLimitMiddleware`

```php
final class Nimbus\Middleware\RateLimitMiddleware implements Pam\Contracts\Http\MiddlewareInterface
```

### Methods

- `public __construct(int $requestsPerSecond, int $burst = 0, int $maxBuckets = 65536, float $idleTtlSeconds = 300, ?Nimbus\RateLimit\RateLimitStore $store = null, ?callable $keyResolver = null): mixed`
- `public process(Pam\Http\Request $request, Pam\Http\Response $response, Pam\Contracts\Http\RequestHandlerInterface $next): Pam\Http\Response`

## `Nimbus\Middleware\ResolveTenantMiddleware`

```php
final readonly class Nimbus\Middleware\ResolveTenantMiddleware implements Pam\Contracts\Http\MiddlewareInterface
```

### Methods

- `public __construct(Nimbus\Tenancy\TenantResolver $resolver, Nimbus\Container\Container $container, bool $required = true): mixed`
- `public process(Pam\Http\Request $request, Pam\Http\Response $response, Pam\Contracts\Http\RequestHandlerInterface $next): Pam\Http\Response`

## `Nimbus\Middleware\ResponseCacheMiddleware`

```php
final readonly class Nimbus\Middleware\ResponseCacheMiddleware implements Pam\Contracts\Http\MiddlewareInterface
```

### Methods

- `public __construct(Nimbus\Cache\ResponseCacheStore $store, int $ttlSeconds = 60, ?callable $keyResolver = null): mixed`
- `public process(Pam\Http\Request $request, Pam\Http\Response $response, Pam\Contracts\Http\RequestHandlerInterface $next): Pam\Http\Response`

## `Nimbus\Middleware\SecurityHeadersMiddleware`

```php
final readonly class Nimbus\Middleware\SecurityHeadersMiddleware implements Pam\Contracts\Http\MiddlewareInterface
```

### Methods

- `public __construct(bool $hsts = true): mixed`
- `public process(Pam\Http\Request $request, Pam\Http\Response $response, Pam\Contracts\Http\RequestHandlerInterface $next): Pam\Http\Response`

## `Nimbus\Middleware\TransactionalMiddleware`

```php
final readonly class Nimbus\Middleware\TransactionalMiddleware implements Pam\Contracts\Http\MiddlewareInterface
```

### Methods

- `public __construct(Nimbus\Transactions\TransactionManager $transactions): mixed`
- `public process(Pam\Http\Request $request, Pam\Http\Response $response, Pam\Contracts\Http\RequestHandlerInterface $next): Pam\Http\Response`

## `Nimbus\Middleware\ValidateSignatureMiddleware`

```php
final readonly class Nimbus\Middleware\ValidateSignatureMiddleware implements Pam\Contracts\Http\MiddlewareInterface
```

### Methods

- `public __construct(Nimbus\Security\UrlSigner $signer): mixed`
- `public process(Pam\Http\Request $request, Pam\Http\Response $response, Pam\Contracts\Http\RequestHandlerInterface $next): Pam\Http\Response`

## `Nimbus\Observability\RequestObservation`

```php
final readonly class Nimbus\Observability\RequestObservation
```

### Properties

- `public readonly float $durationSeconds`
- `public readonly ?string $exceptionClass`
- `public readonly string $method`
- `public readonly string $route`
- `public readonly int $status`

### Methods

- `public __construct(string $method, string $route, int $status, float $durationSeconds, ?string $exceptionClass = null): mixed`

## `Nimbus\Observability\RequestObserver`

```php
abstract interface Nimbus\Observability\RequestObserver
```

### Methods

- `public record(Nimbus\Observability\RequestObservation $observation): void`

## `Nimbus\OpenApi\ClientGenerator`

```php
final readonly class Nimbus\OpenApi\ClientGenerator
```

### Methods

- `public __construct(array $document): mixed`
- `public generate(Nimbus\OpenApi\ClientLanguage $language): string`

## `Nimbus\OpenApi\ClientLanguage`

```php
final enum Nimbus\OpenApi\ClientLanguage implements BackedEnum, UnitEnum: int
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

## `Nimbus\OpenApi\CompatibilityChange`

```php
final readonly class Nimbus\OpenApi\CompatibilityChange
```

### Properties

- `public readonly Nimbus\OpenApi\CompatibilityChangeCode $code`
- `public readonly string $location`
- `public readonly string $message`

### Methods

- `public __construct(Nimbus\OpenApi\CompatibilityChangeCode $code, string $location, string $message): mixed`

## `Nimbus\OpenApi\CompatibilityChangeCode`

```php
final enum Nimbus\OpenApi\CompatibilityChangeCode implements BackedEnum, UnitEnum: int
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

## `Nimbus\OpenApi\CompatibilityChecker`

```php
final class Nimbus\OpenApi\CompatibilityChecker
```

### Methods

- `public breakingChanges(array $previous, array $current): array`

## `Nimbus\OpenApi\OpenApiGenerator`

```php
final readonly class Nimbus\OpenApi\OpenApiGenerator
```

### Methods

- `public __construct(Nimbus\Router $router, string $title = "Nimbus API", string $version = "1.0.0"): mixed`
- `public client(Nimbus\OpenApi\ClientLanguage $language): string`
- `public generate(): array`
- `public toJson(): string`

## `Nimbus\PackageDiscovery`

```php
final class Nimbus\PackageDiscovery
```

### Methods

- `public static providers(string $projectRoot): array`

## `Nimbus\Pagination\Page`

```php
final readonly class Nimbus\Pagination\Page
```

### Properties

- `public readonly array $items`
- `public readonly array $links`
- `public readonly array $meta`

### Methods

- `public __construct(array $items, array $meta, array $links): mixed`

## `Nimbus\Pagination\Paginator`

```php
final readonly class Nimbus\Pagination\Paginator
```

### Constants

- `MAX_PER_PAGE = 100`

### Methods

- `public static cursor(Arcano\ORM\Select $select, Pam\Http\Request $request, int $perPage = 20, string $column = "id", string $property = "id"): Nimbus\Pagination\Page`
- `public static paginate(Arcano\ORM\Select $select, Pam\Http\Request $request, int $perPage = 20): Nimbus\Pagination\Page`

## `Nimbus\PendingRoute`

```php
final readonly class Nimbus\PendingRoute
```

### Methods

- `public __construct(Nimbus\App $app, Nimbus\Router $router, Nimbus\Route $route): mixed`
- `public auth(): self`
- `public can(string $ability): self`
- `public definition(): Nimbus\Route`
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
- `public signed(): self`
- `public summary(string $summary): self`
- `public tags(array $tags): self`
- `public where(string $parameter, Nimbus\RouteConstraint|string $constraint): self`

## `Nimbus\Pipeline`

```php
final class Nimbus\Pipeline implements Pam\Contracts\Http\RequestHandlerInterface
```

### Methods

- `public __construct(array $middleware, Pam\Contracts\Http\RequestHandlerInterface $destination): mixed`
- `public handle(Pam\Http\Request $request, Pam\Http\Response $response): Pam\Http\Response`

## `Nimbus\Profiler\MemoryProfiler`

```php
final class Nimbus\Profiler\MemoryProfiler implements Nimbus\Lifecycle\RequestLifecycleObserver
```

### Methods

- `public __construct(Nimbus\Profiler\ProfilerMode $mode = 1, int $maximumProfiles = 500): mixed`
- `public find(string $token): ?Nimbus\Profiler\RequestProfile`
- `public finished(Pam\Http\Request $request, Pam\Http\Response $response, ?Throwable $failure): void`
- `public recent(int $limit = 50): array`
- `public starting(Pam\Http\Request $request): void`

## `Nimbus\Profiler\ProfilerMode`

```php
final enum Nimbus\Profiler\ProfilerMode implements BackedEnum, UnitEnum: int
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

## `Nimbus\Profiler\RequestProfile`

```php
final readonly class Nimbus\Profiler\RequestProfile
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

## `Nimbus\RateLimit\MemoryRateLimitStore`

```php
final class Nimbus\RateLimit\MemoryRateLimitStore implements Nimbus\RateLimit\RateLimitStore
```

### Methods

- `public __construct(int $maxBuckets = 65536, float $idleTtlSeconds = 300): mixed`
- `public consume(string $key, int $requestsPerSecond, int $capacity, float $now): Nimbus\RateLimit\RateLimitDecision`

## `Nimbus\RateLimit\RateLimitDecision`

```php
final readonly class Nimbus\RateLimit\RateLimitDecision
```

### Properties

- `public readonly bool $allowed`
- `public readonly int $limit`
- `public readonly int $remaining`
- `public readonly int $retryAfterSeconds`

### Methods

- `public __construct(bool $allowed, int $limit, int $remaining, int $retryAfterSeconds = 0): mixed`

## `Nimbus\RateLimit\RateLimitStore`

```php
abstract interface Nimbus\RateLimit\RateLimitStore
```

### Methods

- `public consume(string $key, int $requestsPerSecond, int $capacity, float $now): Nimbus\RateLimit\RateLimitDecision`

## `Nimbus\Redis\Connection`

```php
abstract interface Nimbus\Redis\Connection
```

### Methods

- `public command(array $arguments): mixed`

## `Nimbus\Redis\PamConnection`

```php
final readonly class Nimbus\Redis\PamConnection implements Nimbus\Redis\Connection
```

### Methods

- `public __construct(Pam\Redis\Client $client): mixed`
- `public command(array $arguments): mixed`

## `Nimbus\Redis\RedisIdempotencyStore`

```php
final readonly class Nimbus\Redis\RedisIdempotencyStore implements Nimbus\Idempotency\IdempotencyStore
```

### Methods

- `public __construct(Nimbus\Redis\Connection $redis, string $prefix = "nimbus:idempotency:"): mixed`
- `public get(string $key, int $now): ?Nimbus\Idempotency\IdempotencyRecord`
- `public put(string $key, Nimbus\Idempotency\IdempotencyRecord $record): void`

## `Nimbus\Redis\RedisPool`

```php
final class Nimbus\Redis\RedisPool implements Nimbus\Redis\Connection
```

### Methods

- `public __construct(Closure $connect, int $maxIdle = 16): mixed`
- `public command(array $arguments): mixed`
- `public static fromEnvironment(int $maxIdle = 16): self`

## `Nimbus\Redis\RedisRateLimitStore`

```php
final readonly class Nimbus\Redis\RedisRateLimitStore implements Nimbus\RateLimit\RateLimitStore
```

### Methods

- `public __construct(Nimbus\Redis\Connection $redis, string $prefix = "nimbus:rate:"): mixed`
- `public consume(string $key, int $requestsPerSecond, int $capacity, float $now): Nimbus\RateLimit\RateLimitDecision`

## `Nimbus\Redis\RedisResponseCacheStore`

```php
final readonly class Nimbus\Redis\RedisResponseCacheStore implements Nimbus\Cache\ResponseCacheStore
```

### Methods

- `public __construct(Nimbus\Redis\Connection $redis, string $prefix = "nimbus:response:"): mixed`
- `public forget(string $key): void`
- `public get(string $key, int $now): ?Nimbus\Cache\CacheRecord`
- `public put(string $key, Nimbus\Cache\CacheRecord $record): void`

## `Nimbus\Resilience\CircuitBreaker`

```php
final class Nimbus\Resilience\CircuitBreaker
```

### Methods

- `public __construct(int $failureThreshold = 5, int $cooldownSeconds = 30): mixed`
- `public call(callable $operation): mixed`
- `public state(): Nimbus\Resilience\CircuitState`

## `Nimbus\Resilience\CircuitOpenException`

```php
final class Nimbus\Resilience\CircuitOpenException extends RuntimeException implements Stringable, Throwable
```

## `Nimbus\Resilience\CircuitState`

```php
final enum Nimbus\Resilience\CircuitState implements BackedEnum, UnitEnum: int
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

## `Nimbus\Resilience\RetryPolicy`

```php
final readonly class Nimbus\Resilience\RetryPolicy
```

### Methods

- `public __construct(int $attempts = 3, int $initialDelayMilliseconds = 10, ?callable $when = null): mixed`
- `public run(callable $operation): mixed`

## `Nimbus\Route`

```php
final class Nimbus\Route
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

## `Nimbus\RouteConstraint`

```php
final enum Nimbus\RouteConstraint implements BackedEnum, UnitEnum: int
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

## `Nimbus\RouteRegistrar`

```php
final class Nimbus\RouteRegistrar
```

### Methods

- `public __construct(Nimbus\App $app, string $prefix = ""): mixed`
- `public apiResource(string $path, string $controller): void`
- `public auth(): self`
- `public can(string $ability): self`
- `public controller(string $controller, callable $routes): void`
- `public delete(string $path, array|callable|string $handler): Nimbus\PendingRoute`
- `public get(string $path, array|callable|string $handler): Nimbus\PendingRoute`
- `public group(callable $routes): void`
- `public head(string $path, array|callable|string $handler): Nimbus\PendingRoute`
- `public load(string $file): void`
- `public middleware(Pam\Contracts\Http\MiddlewareInterface|array|callable|string $middleware): self`
- `public options(string $path, array|callable|string $handler): Nimbus\PendingRoute`
- `public patch(string $path, array|callable|string $handler): Nimbus\PendingRoute`
- `public post(string $path, array|callable|string $handler): Nimbus\PendingRoute`
- `public prefix(string $prefix): self`
- `public put(string $path, array|callable|string $handler): Nimbus\PendingRoute`
- `public resource(string $path, string $controller, ?callable $routes = null): void`
- `public signed(): self`

## `Nimbus\Router`

```php
final class Nimbus\Router
```

### Methods

- `public __construct(?Nimbus\Container\Container $container = null, int $maximumRoutes = 10000): mixed`
- `public add(string $method, string $path, callable $handler): self`
- `public constrain(Nimbus\Route $route, string $parameter, Nimbus\RouteConstraint|string $constraint): void`
- `public container(): Nimbus\Container\Container`
- `public match(string $method, string $path): Nimbus\RoutingResult`
- `public name(Nimbus\Route $route, string $name): void`
- `public register(string $method, string $path, callable $handler): Nimbus\Route`
- `public routes(): array`

## `Nimbus\RoutingResult`

```php
final readonly class Nimbus\RoutingResult
```

### Properties

- `public readonly array $allowedMethods`
- `public readonly array $parameters`
- `public readonly ?Nimbus\Route $route`
- `public readonly Nimbus\RoutingResultType $type`

### Methods

- `public __construct(Nimbus\RoutingResultType $type, ?Nimbus\Route $route = null, array $parameters = [], array $allowedMethods = []): mixed`

## `Nimbus\RoutingResultType`

```php
final enum Nimbus\RoutingResultType implements BackedEnum, UnitEnum: int
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

## `Nimbus\Routing\RouteBindable`

```php
abstract interface Nimbus\Routing\RouteBindable
```

### Methods

- `public static resolveRouteBinding(string $value): static`

## `Nimbus\Runtime\Background`

```php
final class Nimbus\Runtime\Background
```

### Methods

- `public __construct(?Closure $clock = null, ?Closure $onFailure = null): mixed`
- `public add(string $name, callable $task): self`
- `public answer(Pam\Http\Request $request, Pam\Http\Response $response): ?Pam\Http\Response`
- `public queue(Nimbus\Jobs\JobWorker $worker, string $name = "queue", int $maximumJobs = 100, float $maximumSeconds = 0.8): self`
- `public run(float $intervalSeconds = 1, ?int $maximumTicks = null): void`
- `public scheduler(Nimbus\Scheduling\Scheduler $scheduler, string $name = "scheduler"): self`
- `public stop(): void`
- `public tick(): void`

## `Nimbus\Runtime\Deadline`

```php
final readonly class Nimbus\Runtime\Deadline
```

### Methods

- `public __construct(float $seconds): mixed`
- `public expired(): bool`
- `public throwIfExpired(): void`

## `Nimbus\Scheduling\DatabaseSchedulerLock`

```php
final class Nimbus\Scheduling\DatabaseSchedulerLock implements Nimbus\Scheduling\SchedulerLock
```

### Methods

- `public __construct(Arcano\Database\DatabaseInterface|Nimbus\Database\ArcanoDatabaseProvider $database, ?string $connection = null, ?Closure $clock = null): mixed`
- `public acquire(string $key, int $seconds): bool`
- `public release(string $key): void`

## `Nimbus\Scheduling\MemorySchedulerLock`

```php
final class Nimbus\Scheduling\MemorySchedulerLock implements Nimbus\Scheduling\SchedulerLock
```

### Methods

- `public __construct(?Closure $clock = null): mixed`
- `public acquire(string $key, int $seconds): bool`
- `public release(string $key): void`

## `Nimbus\Scheduling\RedisSchedulerLock`

```php
final class Nimbus\Scheduling\RedisSchedulerLock implements Nimbus\Scheduling\SchedulerLock
```

### Methods

- `public __construct(Nimbus\Redis\Connection $redis, string $prefix = "nimbus:lock:"): mixed`
- `public acquire(string $key, int $seconds): bool`
- `public release(string $key): void`

## `Nimbus\Scheduling\Schedule`

```php
final class Nimbus\Scheduling\Schedule
```

### Methods

- `public __construct(?Nimbus\Jobs\JobDispatcher $dispatcher = null, DateTimeZone|null|string $timezone = null, ?Nimbus\Container\Container $container = null): mixed`
- `public call(callable $callback): Nimbus\Scheduling\ScheduledTask`
- `public dueTasks(DateTimeInterface $now): array`
- `public job(object|string $job, int $maximumAttempts = 3): Nimbus\Scheduling\ScheduledTask`
- `public tasks(): array`

## `Nimbus\Scheduling\ScheduledTask`

```php
final class Nimbus\Scheduling\ScheduledTask
```

### Methods

- `public __construct(Closure $action, string $name, ?DateTimeZone $timezone = null): mixed`
- `public static assertCronAvailable(): void`
- `public cron(string $expression): self`
- `public daily(): self`
- `public dailyAt(string $time): self`
- `public everyFifteenMinutes(): self`
- `public everyFiveMinutes(): self`
- `public everyMinute(): self`
- `public everyTenMinutes(): self`
- `public everyThirtyMinutes(): self`
- `public everyTwoMinutes(): self`
- `public expression(): string`
- `public getName(): string`
- `public hourly(): self`
- `public hourlyAt(int $minute): self`
- `public isDue(DateTimeInterface $now): bool`
- `public monthly(): self`
- `public monthlyOn(int $day = 1, string $time = "00:00"): self`
- `public name(string $name): self`
- `public overlapMinutes(): ?int`
- `public run(): void`
- `public timezone(DateTimeZone|string $timezone): self`
- `public weekly(): self`
- `public weeklyOn(int $day, string $time = "00:00"): self`
- `public withoutOverlapping(int $expiresAfterMinutes = 1440): self`

## `Nimbus\Scheduling\Scheduler`

```php
final class Nimbus\Scheduling\Scheduler
```

### Methods

- `public __construct(Nimbus\Scheduling\Schedule $schedule, Nimbus\Scheduling\SchedulerLock $lock = {"@object":"Nimbus\\Scheduling\\MemorySchedulerLock","properties":[]}, ?Closure $clock = null, ?Closure $onFailure = null): mixed`
- `public runDue(?int $now = null): array`

## `Nimbus\Scheduling\SchedulerLock`

```php
abstract interface Nimbus\Scheduling\SchedulerLock
```

### Methods

- `public acquire(string $key, int $seconds): bool`
- `public release(string $key): void`

## `Nimbus\Security\Crypt`

```php
final readonly class Nimbus\Security\Crypt
```

### Methods

- `public __construct(string $key): mixed`
- `public decrypt(string $payload): string`
- `public encrypt(string $plaintext): string`
- `public static fromEnvironment(): self`
- `public static generateKey(): string`
- `public static keyFromEnvironment(): string`

## `Nimbus\Security\DecryptException`

```php
final class Nimbus\Security\DecryptException extends RuntimeException implements Stringable, Throwable
```

## `Nimbus\Security\Hash`

```php
final readonly class Nimbus\Security\Hash
```

### Methods

- `public static check(string $password, string $hash): bool`
- `public static make(string $password): string`
- `public static needsRehash(string $hash): bool`

## `Nimbus\Security\UrlSignatureState`

```php
final enum Nimbus\Security\UrlSignatureState implements BackedEnum, UnitEnum: int
```

### Enum cases

- `Valid = 1`
- `Invalid = 2`
- `Expired = 3`

### Constants

- `Expired = 3`
- `Invalid = 2`
- `Valid = 1`

### Properties

- `public readonly string $name`
- `public readonly int $value`

### Methods

- `public static cases(): array`
- `public static from(int|string $value): static`
- `public static tryFrom(int|string $value): ?static`

## `Nimbus\Security\UrlSigner`

```php
final readonly class Nimbus\Security\UrlSigner
```

### Constants

- `EXPIRES = "expires"`
- `SIGNATURE = "signature"`

### Methods

- `public __construct(string $key): mixed`
- `public check(string $path, array $query, ?int $now = null): Nimbus\Security\UrlSignatureState`
- `public static fromEnvironment(): self`
- `public sign(string $path, array $query = [], ?int $expiresAt = null): string`
- `public temporary(string $path, int $ttl, array $query = []): string`
- `public verify(string $path, array $query, ?int $now = null): bool`

## `Nimbus\Storage\Disk`

```php
final readonly class Nimbus\Storage\Disk
```

### Methods

- `public __construct(string $name, League\Flysystem\FilesystemOperator $filesystem, ?Nimbus\Storage\UploadUrlGenerator $uploads = null): mixed`
- `public delete(string $path): void`
- `public exists(string $path): bool`
- `public filesystem(): League\Flysystem\FilesystemOperator`
- `public get(string $path): string`
- `public mimeType(string $path): string`
- `public name(): string`
- `public put(string $path, string $contents, array $config = []): void`
- `public readStream(string $path): mixed`
- `public size(string $path): int`
- `public temporaryUploadUrl(string $path, int $ttl, int $maxBytes, ?string $contentType = null): string`
- `public temporaryUrl(string $path, int $ttl): string`
- `public url(string $path): string`
- `public writeStream(string $path, mixed $stream, array $config = []): void`

## `Nimbus\Storage\S3UploadUrlGenerator`

```php
final readonly class Nimbus\Storage\S3UploadUrlGenerator implements Nimbus\Storage\UploadUrlGenerator
```

### Methods

- `public __construct(object $client, string $bucket, string $prefix = ""): mixed`
- `public temporaryUploadUrl(string $path, DateTimeImmutable $expiresAt, int $maxBytes, ?string $contentType = null): string`

## `Nimbus\Storage\SignedStorageUrls`

```php
final readonly class Nimbus\Storage\SignedStorageUrls implements League\Flysystem\UrlGeneration\TemporaryUrlGenerator, Nimbus\Storage\UploadUrlGenerator
```

### Methods

- `public __construct(Nimbus\Security\UrlSigner $signer, string $disk, string $routes, string $baseUrl = ""): mixed`
- `public temporaryUploadUrl(string $path, DateTimeImmutable $expiresAt, int $maxBytes, ?string $contentType = null): string`
- `public temporaryUrl(string $path, DateTimeInterface $expiresAt, League\Flysystem\Config $config): string`

## `Nimbus\Storage\StorageConfig`

```php
final readonly class Nimbus\Storage\StorageConfig
```

### Properties

- `public readonly string $defaultDisk`
- `public readonly array $disks`
- `public readonly string $url`

### Methods

- `public __construct(string $defaultDisk, array $disks, string $url = ""): mixed`
- `public disk(string $name): array`
- `public static fromEnvironment(): self`

## `Nimbus\Storage\StorageManager`

```php
final class Nimbus\Storage\StorageManager
```

### Methods

- `public __construct(Nimbus\Storage\StorageConfig $config, ?Nimbus\Security\UrlSigner $signer = null, ?string $routes = null): mixed`
- `public disk(?string $name = null): Nimbus\Storage\Disk`
- `public filesystem(string $name, League\Flysystem\FilesystemOperator $filesystem): Nimbus\Storage\Disk`
- `public register(Nimbus\Storage\Disk $disk): self`

## `Nimbus\Storage\StorageRoutes`

```php
final readonly class Nimbus\Storage\StorageRoutes
```

### Methods

- `public __construct(Nimbus\Storage\StorageManager $storage): mixed`
- `public download(Pam\Http\Request $request, Pam\Http\Response $response): Pam\Http\Response`
- `public static downloadPath(string $path): string`
- `public register(Nimbus\App $app, string $path): void`
- `public upload(Pam\Http\Request $request, Pam\Http\Response $response): Pam\Http\Response`
- `public static uploadPath(string $path): string`

## `Nimbus\Storage\StorageServiceProvider`

```php
final readonly class Nimbus\Storage\StorageServiceProvider implements Pam\Contracts\Package\ServiceProviderInterface
```

### Methods

- `public __construct(Nimbus\Storage\StorageConfig $config, ?string $routes = null, ?Nimbus\Security\UrlSigner $signer = null): mixed`
- `public boot(Pam\Contracts\Http\ApplicationInterface $application): void`
- `public register(Pam\Contracts\Http\ApplicationInterface $application): void`

## `Nimbus\Storage\UploadUrlGenerator`

```php
abstract interface Nimbus\Storage\UploadUrlGenerator
```

### Methods

- `public temporaryUploadUrl(string $path, DateTimeImmutable $expiresAt, int $maxBytes, ?string $contentType = null): string`

## `Nimbus\Tenancy\Tenant`

```php
abstract interface Nimbus\Tenancy\Tenant
```

### Methods

- `public identifier(): string`

## `Nimbus\Tenancy\TenantContext`

```php
final readonly class Nimbus\Tenancy\TenantContext
```

### Properties

- `public readonly Nimbus\Tenancy\Tenant $tenant`

### Methods

- `public __construct(Nimbus\Tenancy\Tenant $tenant): mixed`

## `Nimbus\Tenancy\TenantResolver`

```php
abstract interface Nimbus\Tenancy\TenantResolver
```

### Methods

- `public resolve(Pam\Http\Request $request): ?Nimbus\Tenancy\Tenant`

## `Nimbus\Testing\ActingAsAuthenticator`

```php
final readonly class Nimbus\Testing\ActingAsAuthenticator implements Nimbus\Auth\Authenticator
```

### Properties

- `public readonly Nimbus\Auth\Principal $principal`

### Methods

- `public __construct(Nimbus\Auth\Principal $principal): mixed`
- `public authenticate(Pam\Http\Request $request): Nimbus\Auth\Principal`

## `Nimbus\Testing\TestCase`

```php
abstract class Nimbus\Testing\TestCase extends PHPUnit\Framework\TestCase implements Countable, PHPUnit\Framework\Reorderable, PHPUnit\Framework\SelfDescribing, PHPUnit\Framework\Test
```

## `Nimbus\Testing\TestClient`

```php
final readonly class Nimbus\Testing\TestClient
```

### Methods

- `public __construct(Nimbus\App $app, array $headers = []): mixed`
- `public actingAs(Nimbus\Auth\Principal $principal): self`
- `public deleteJson(string $path, ?array $payload = null, array $headers = []): Nimbus\Testing\TestResponse`
- `public get(string $path, array $headers = []): Nimbus\Testing\TestResponse`
- `public getJson(string $path, array $headers = []): Nimbus\Testing\TestResponse`
- `public json(string $method, string $path, ?array $payload = null, array $headers = []): Nimbus\Testing\TestResponse`
- `public patchJson(string $path, array $payload = [], array $headers = []): Nimbus\Testing\TestResponse`
- `public postJson(string $path, array $payload = [], array $headers = []): Nimbus\Testing\TestResponse`
- `public putJson(string $path, array $payload = [], array $headers = []): Nimbus\Testing\TestResponse`
- `public request(string $method, string $path, array $query = [], array $headers = [], string $body = ""): Nimbus\Testing\TestResponse`
- `public withHeader(string $name, string $value): self`
- `public withHeaders(array $headers): self`
- `public withToken(string $token, string $type = "Bearer"): self`

## `Nimbus\Testing\TestResponse`

```php
final readonly class Nimbus\Testing\TestResponse
```

### Methods

- `public __construct(array $response): mixed`
- `public assertCreated(): self`
- `public assertExactJson(array $expected): self`
- `public assertForbidden(): self`
- `public assertHeader(string $name, ?string $expected = null): self`
- `public assertHeaderMissing(string $name): self`
- `public assertJson(array $expected): self`
- `public assertJsonCount(int $count, ?string $path = null): self`
- `public assertJsonMissingPath(string $path): self`
- `public assertJsonMissingValidationErrors(array|null|string $fields = null): self`
- `public assertJsonPath(string $path, mixed $expected): self`
- `public assertJsonStructure(array $structure, ?string $path = null): self`
- `public assertJsonValidationErrors(array|string $fields): self`
- `public assertNoContent(): self`
- `public assertNotFound(): self`
- `public assertOk(): self`
- `public assertStatus(int $expected): self`
- `public assertSuccessful(): self`
- `public assertUnauthorized(): self`
- `public assertUnprocessableEntity(): self`
- `public body(): string`
- `public dump(): self`
- `public header(string $name): ?string`
- `public json(?string $path = null): mixed`
- `public status(): int`

## `Nimbus\Transactions\TransactionManager`

```php
abstract interface Nimbus\Transactions\TransactionManager
```

### Methods

- `public transaction(callable $operation): mixed`

## `Nimbus\Validation\Attributes\Email`

```php
final readonly class Nimbus\Validation\Attributes\Email implements Nimbus\Validation\ValidationRule
```

### Methods

- `public validate(string $field, mixed $value): ?string`

## `Nimbus\Validation\Attributes\Field`

```php
final readonly class Nimbus\Validation\Attributes\Field
```

### Properties

- `public readonly string $name`

### Methods

- `public __construct(string $name): mixed`

## `Nimbus\Validation\Attributes\In`

```php
final readonly class Nimbus\Validation\Attributes\In implements Nimbus\Validation\ValidationRule
```

### Properties

- `public readonly array $values`

### Methods

- `public __construct(bool|float|int|string ...$values): mixed`
- `public validate(string $field, mixed $value): ?string`

## `Nimbus\Validation\Attributes\Max`

```php
final readonly class Nimbus\Validation\Attributes\Max implements Nimbus\Validation\ValidationRule
```

### Properties

- `public readonly float|int $value`

### Methods

- `public __construct(float|int $value): mixed`
- `public validate(string $field, mixed $value): ?string`

## `Nimbus\Validation\Attributes\Min`

```php
final readonly class Nimbus\Validation\Attributes\Min implements Nimbus\Validation\ValidationRule
```

### Properties

- `public readonly float|int $value`

### Methods

- `public __construct(float|int $value): mixed`
- `public validate(string $field, mixed $value): ?string`

## `Nimbus\Validation\Attributes\Regex`

```php
final readonly class Nimbus\Validation\Attributes\Regex implements Nimbus\Validation\ValidationRule
```

### Properties

- `public readonly string $pattern`

### Methods

- `public __construct(string $pattern): mixed`
- `public validate(string $field, mixed $value): ?string`

## `Nimbus\Validation\Attributes\Url`

```php
final readonly class Nimbus\Validation\Attributes\Url implements Nimbus\Validation\ValidationRule
```

### Methods

- `public validate(string $field, mixed $value): ?string`

## `Nimbus\Validation\Attributes\Uuid`

```php
final readonly class Nimbus\Validation\Attributes\Uuid implements Nimbus\Validation\ValidationRule
```

### Methods

- `public validate(string $field, mixed $value): ?string`

## `Nimbus\Validation\DtoHydrator`

```php
final class Nimbus\Validation\DtoHydrator
```

### Methods

- `public hydrate(string $class, array $data): object`

## `Nimbus\Validation\EnumRule`

```php
final readonly class Nimbus\Validation\EnumRule implements Nimbus\Validation\ValidationRule
```

### Methods

- `public __construct(string $enum): mixed`
- `public validate(string $field, mixed $value): ?string`

## `Nimbus\Validation\FormRequest`

```php
abstract class Nimbus\Validation\FormRequest
```

### Methods

- `public __construct(Pam\Http\Request $request): mixed`
- `public authorize(): bool`
- `public dto(string $class): object`
- `public input(string $key, mixed $default = null): mixed`
- `public rules(): array`
- `public validated(): array`

## `Nimbus\Validation\Input`

```php
abstract class Nimbus\Validation\Input
```

### Methods

- `public __construct(Pam\Http\Request $request): mixed`
- `public authorize(Pam\Http\Request $request): bool`
- `public static from(array $data): static`
- `public static nested(array $data, string $prefix, array &$errors): static`
- `public static schema(): array`
- `public toArray(): array`

## `Nimbus\Validation\Rule`

```php
final class Nimbus\Validation\Rule
```

### Methods

- `public static enum(string $enum): Nimbus\Validation\EnumRule`

## `Nimbus\Validation\ValidationException`

```php
final class Nimbus\Validation\ValidationException extends Nimbus\Http\HttpException implements Stringable, Throwable
```

### Properties

- `public readonly array $errors`

### Methods

- `public __construct(array $errors): mixed`

## `Nimbus\Validation\ValidationRule`

```php
abstract interface Nimbus\Validation\ValidationRule
```

### Methods

- `public validate(string $field, mixed $value): ?string`
