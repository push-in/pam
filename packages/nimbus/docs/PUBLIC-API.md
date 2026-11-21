# Nimbus public reference

> Generated from `api-surface.json` by `bin/api-reference`. Do not edit manually.

Every symbol below is protected by the Nimbus Semantic Versioning gate.

## Symbols

- [`Nimbus\Admin\Admin`](#nimbusadminadmin)
- [`Nimbus\Admin\AdminAction`](#nimbusadminadminaction)
- [`Nimbus\Admin\AdminField`](#nimbusadminadminfield)
- [`Nimbus\Admin\AdminPanel`](#nimbusadminadminpanel)
- [`Nimbus\Admin\AdminResource`](#nimbusadminadminresource)
- [`Nimbus\App`](#nimbusapp)
- [`Nimbus\Auth\Abilities`](#nimbusauthabilities)
- [`Nimbus\Auth\AccessTokenIssuer`](#nimbusauthaccesstokenissuer)
- [`Nimbus\Auth\AccessTokenVerifier`](#nimbusauthaccesstokenverifier)
- [`Nimbus\Auth\AuthContext`](#nimbusauthauthcontext)
- [`Nimbus\Auth\AuthRoutes`](#nimbusauthauthroutes)
- [`Nimbus\Auth\AuthTablesMigration`](#nimbusauthauthtablesmigration)
- [`Nimbus\Auth\AuthUser`](#nimbusauthauthuser)
- [`Nimbus\Auth\AuthUserProvider`](#nimbusauthauthuserprovider)
- [`Nimbus\Auth\Authenticator`](#nimbusauthauthenticator)
- [`Nimbus\Auth\BearerTokenAuthenticator`](#nimbusauthbearertokenauthenticator)
- [`Nimbus\Auth\ChainAuthenticator`](#nimbusauthchainauthenticator)
- [`Nimbus\Auth\DatabaseTokenRevocationStore`](#nimbusauthdatabasetokenrevocationstore)
- [`Nimbus\Auth\EmailVerification`](#nimbusauthemailverification)
- [`Nimbus\Auth\Gate`](#nimbusauthgate)
- [`Nimbus\Auth\HmacTokenCodec`](#nimbusauthhmactokencodec)
- [`Nimbus\Auth\JwtAuthenticator`](#nimbusauthjwtauthenticator)
- [`Nimbus\Auth\JwtCodec`](#nimbusauthjwtcodec)
- [`Nimbus\Auth\LoginTokenIssuer`](#nimbusauthlogintokenissuer)
- [`Nimbus\Auth\MemoryTokenRevocationStore`](#nimbusauthmemorytokenrevocationstore)
- [`Nimbus\Auth\NewPersonalAccessToken`](#nimbusauthnewpersonalaccesstoken)
- [`Nimbus\Auth\NewRefreshToken`](#nimbusauthnewrefreshtoken)
- [`Nimbus\Auth\OAuth\DatabaseOAuthStateStore`](#nimbusauthoauthdatabaseoauthstatestore)
- [`Nimbus\Auth\OAuth\MemoryOAuthStateStore`](#nimbusauthoauthmemoryoauthstatestore)
- [`Nimbus\Auth\OAuth\OAuthStateStore`](#nimbusauthoauthoauthstatestore)
- [`Nimbus\Auth\OAuth\SocialIdentity`](#nimbusauthoauthsocialidentity)
- [`Nimbus\Auth\OAuth\SocialLogin`](#nimbusauthoauthsociallogin)
- [`Nimbus\Auth\OAuth\SocialUserResolver`](#nimbusauthoauthsocialuserresolver)
- [`Nimbus\Auth\OneTimeTokens`](#nimbusauthonetimetokens)
- [`Nimbus\Auth\PasswordResets`](#nimbusauthpasswordresets)
- [`Nimbus\Auth\PersonalAccessToken`](#nimbusauthpersonalaccesstoken)
- [`Nimbus\Auth\PersonalAccessTokenAuthenticator`](#nimbusauthpersonalaccesstokenauthenticator)
- [`Nimbus\Auth\PersonalAccessTokens`](#nimbusauthpersonalaccesstokens)
- [`Nimbus\Auth\Principal`](#nimbusauthprincipal)
- [`Nimbus\Auth\RefreshToken`](#nimbusauthrefreshtoken)
- [`Nimbus\Auth\RefreshTokenReusedException`](#nimbusauthrefreshtokenreusedexception)
- [`Nimbus\Auth\RefreshTokens`](#nimbusauthrefreshtokens)
- [`Nimbus\Auth\ResetPasswordNotification`](#nimbusauthresetpasswordnotification)
- [`Nimbus\Auth\RevokesCredentials`](#nimbusauthrevokescredentials)
- [`Nimbus\Auth\SessionAuthenticator`](#nimbusauthsessionauthenticator)
- [`Nimbus\Auth\SessionRevocations`](#nimbusauthsessionrevocations)
- [`Nimbus\Auth\TokenPair`](#nimbusauthtokenpair)
- [`Nimbus\Auth\TokenPairIssuer`](#nimbusauthtokenpairissuer)
- [`Nimbus\Auth\TokenPrincipal`](#nimbusauthtokenprincipal)
- [`Nimbus\Auth\TokenRevocationStore`](#nimbusauthtokenrevocationstore)
- [`Nimbus\Auth\VerifyEmailNotification`](#nimbusauthverifyemailnotification)
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
- [`Nimbus\Http\Cookie`](#nimbushttpcookie)
- [`Nimbus\Http\CookieCodec`](#nimbushttpcookiecodec)
- [`Nimbus\Http\CookieJar`](#nimbushttpcookiejar)
- [`Nimbus\Http\FormData`](#nimbushttpformdata)
- [`Nimbus\Http\HttpException`](#nimbushttphttpexception)
- [`Nimbus\Http\JsonResource`](#nimbushttpjsonresource)
- [`Nimbus\Http\ProblemCode`](#nimbushttpproblemcode)
- [`Nimbus\Http\Redirect`](#nimbushttpredirect)
- [`Nimbus\Http\RequestCookies`](#nimbushttprequestcookies)
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
- [`Nimbus\Middleware\EncryptCookiesMiddleware`](#nimbusmiddlewareencryptcookiesmiddleware)
- [`Nimbus\Middleware\IdempotencyMiddleware`](#nimbusmiddlewareidempotencymiddleware)
- [`Nimbus\Middleware\MethodOverrideMiddleware`](#nimbusmiddlewaremethodoverridemiddleware)
- [`Nimbus\Middleware\ObserveRequestMiddleware`](#nimbusmiddlewareobserverequestmiddleware)
- [`Nimbus\Middleware\QueryBudgetMiddleware`](#nimbusmiddlewarequerybudgetmiddleware)
- [`Nimbus\Middleware\RateLimitMiddleware`](#nimbusmiddlewareratelimitmiddleware)
- [`Nimbus\Middleware\ResolveTenantMiddleware`](#nimbusmiddlewareresolvetenantmiddleware)
- [`Nimbus\Middleware\ResponseCacheMiddleware`](#nimbusmiddlewareresponsecachemiddleware)
- [`Nimbus\Middleware\SecurityHeadersMiddleware`](#nimbusmiddlewaresecurityheadersmiddleware)
- [`Nimbus\Middleware\StartSessionMiddleware`](#nimbusmiddlewarestartsessionmiddleware)
- [`Nimbus\Middleware\TransactionalMiddleware`](#nimbusmiddlewaretransactionalmiddleware)
- [`Nimbus\Middleware\ValidateSignatureMiddleware`](#nimbusmiddlewarevalidatesignaturemiddleware)
- [`Nimbus\Middleware\VerifyCsrfTokenMiddleware`](#nimbusmiddlewareverifycsrftokenmiddleware)
- [`Nimbus\Notifications\Database\DatabaseChannel`](#nimbusnotificationsdatabasedatabasechannel)
- [`Nimbus\Notifications\Database\DatabaseNotification`](#nimbusnotificationsdatabasedatabasenotification)
- [`Nimbus\Notifications\Database\DatabaseNotifications`](#nimbusnotificationsdatabasedatabasenotifications)
- [`Nimbus\Notifications\Http\CurlHttpClient`](#nimbusnotificationshttpcurlhttpclient)
- [`Nimbus\Notifications\Http\HttpClient`](#nimbusnotificationshttphttpclient)
- [`Nimbus\Notifications\Http\HttpClients`](#nimbusnotificationshttphttpclients)
- [`Nimbus\Notifications\Http\HttpRequest`](#nimbusnotificationshttphttprequest)
- [`Nimbus\Notifications\Http\HttpResponse`](#nimbusnotificationshttphttpresponse)
- [`Nimbus\Notifications\Http\HttpTransportException`](#nimbusnotificationshttphttptransportexception)
- [`Nimbus\Notifications\Http\PamHttpClient`](#nimbusnotificationshttppamhttpclient)
- [`Nimbus\Notifications\Http\Psr18HttpClient`](#nimbusnotificationshttppsr18httpclient)
- [`Nimbus\Notifications\Mail\MailChannel`](#nimbusnotificationsmailmailchannel)
- [`Nimbus\Notifications\Mail\MailMessage`](#nimbusnotificationsmailmailmessage)
- [`Nimbus\Notifications\Notifiable`](#nimbusnotificationsnotifiable)
- [`Nimbus\Notifications\Notification`](#nimbusnotificationsnotification)
- [`Nimbus\Notifications\NotificationChannel`](#nimbusnotificationsnotificationchannel)
- [`Nimbus\Notifications\NotificationDeliveryException`](#nimbusnotificationsnotificationdeliveryexception)
- [`Nimbus\Notifications\NotificationDispatcher`](#nimbusnotificationsnotificationdispatcher)
- [`Nimbus\Notifications\NotificationFake`](#nimbusnotificationsnotificationfake)
- [`Nimbus\Notifications\NotificationSender`](#nimbusnotificationsnotificationsender)
- [`Nimbus\Notifications\Notifier`](#nimbusnotificationsnotifier)
- [`Nimbus\Notifications\OnDemandNotifiable`](#nimbusnotificationsondemandnotifiable)
- [`Nimbus\Notifications\Push\ApnsProvider`](#nimbusnotificationspushapnsprovider)
- [`Nimbus\Notifications\Push\FcmProvider`](#nimbusnotificationspushfcmprovider)
- [`Nimbus\Notifications\Push\JsonWebToken`](#nimbusnotificationspushjsonwebtoken)
- [`Nimbus\Notifications\Push\PushChannel`](#nimbusnotificationspushpushchannel)
- [`Nimbus\Notifications\Push\PushMessage`](#nimbusnotificationspushpushmessage)
- [`Nimbus\Notifications\Push\PushOutcome`](#nimbusnotificationspushpushoutcome)
- [`Nimbus\Notifications\Push\PushPlatform`](#nimbusnotificationspushpushplatform)
- [`Nimbus\Notifications\Push\PushProvider`](#nimbusnotificationspushpushprovider)
- [`Nimbus\Notifications\Push\PushTarget`](#nimbusnotificationspushpushtarget)
- [`Nimbus\Notifications\SendNotificationHandler`](#nimbusnotificationssendnotificationhandler)
- [`Nimbus\Notifications\SendNotificationJob`](#nimbusnotificationssendnotificationjob)
- [`Nimbus\Notifications\Webhook\WebhookChannel`](#nimbusnotificationswebhookwebhookchannel)
- [`Nimbus\Notifications\Webhook\WebhookMessage`](#nimbusnotificationswebhookwebhookmessage)
- [`Nimbus\Notifications\Webhook\WebhookSignature`](#nimbusnotificationswebhookwebhooksignature)
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
- [`Nimbus\Redis\RedisOAuthStateStore`](#nimbusredisredisoauthstatestore)
- [`Nimbus\Redis\RedisPool`](#nimbusredisredispool)
- [`Nimbus\Redis\RedisRateLimitStore`](#nimbusredisredisratelimitstore)
- [`Nimbus\Redis\RedisResponseCacheStore`](#nimbusredisredisresponsecachestore)
- [`Nimbus\Redis\RedisSessionStore`](#nimbusredisredissessionstore)
- [`Nimbus\Redis\RedisTokenRevocationStore`](#nimbusredisredistokenrevocationstore)
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
- [`Nimbus\Session\DatabaseSessionStore`](#nimbussessiondatabasesessionstore)
- [`Nimbus\Session\ErrorBag`](#nimbussessionerrorbag)
- [`Nimbus\Session\FileSessionStore`](#nimbussessionfilesessionstore)
- [`Nimbus\Session\MemorySessionStore`](#nimbussessionmemorysessionstore)
- [`Nimbus\Session\Session`](#nimbussessionsession)
- [`Nimbus\Session\SessionConfig`](#nimbussessionsessionconfig)
- [`Nimbus\Session\SessionServiceProvider`](#nimbussessionsessionserviceprovider)
- [`Nimbus\Session\SessionStore`](#nimbussessionsessionstore)
- [`Nimbus\Session\SessionTablesMigration`](#nimbussessionsessiontablesmigration)
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
- [`Nimbus\Translation\LangDirectoryLoader`](#nimbustranslationlangdirectoryloader)
- [`Nimbus\Translation\Locale`](#nimbustranslationlocale)
- [`Nimbus\Translation\LocaleMiddleware`](#nimbustranslationlocalemiddleware)
- [`Nimbus\Translation\TranslationServiceProvider`](#nimbustranslationtranslationserviceprovider)
- [`Nimbus\Translation\Translator`](#nimbustranslationtranslator)
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
- [`Nimbus\View\AttributeBag`](#nimbusviewattributebag)
- [`Nimbus\View\CompiledTemplate`](#nimbusviewcompiledtemplate)
- [`Nimbus\View\Compiler`](#nimbusviewcompiler)
- [`Nimbus\View\Component`](#nimbusviewcomponent)
- [`Nimbus\View\HtmlString`](#nimbusviewhtmlstring)
- [`Nimbus\View\Htmlable`](#nimbusviewhtmlable)
- [`Nimbus\View\Loop`](#nimbusviewloop)
- [`Nimbus\View\Slot`](#nimbusviewslot)
- [`Nimbus\View\View`](#nimbusviewview)
- [`Nimbus\View\ViewChecker`](#nimbusviewviewchecker)
- [`Nimbus\View\ViewException`](#nimbusviewviewexception)
- [`Nimbus\View\ViewFactory`](#nimbusviewviewfactory)
- [`Nimbus\View\ViewServiceProvider`](#nimbusviewviewserviceprovider)

## `Nimbus\Admin\Admin`

```php
final class Nimbus\Admin\Admin
```

### Methods

- `public static panel(Nimbus\App $app, string $prefix = "/admin", ?string $loginUrl = null, string $title = "Admin", ?string $views = null): Nimbus\Admin\AdminPanel`

## `Nimbus\Admin\AdminAction`

```php
final readonly class Nimbus\Admin\AdminAction
```

### Properties

- `public readonly string $ability`
- `public readonly bool $destructive`
- `public readonly Closure $handler`
- `public readonly string $label`

### Methods

- `public __construct(string $label, Closure $handler, string $ability = "update", bool $destructive = false): mixed`

## `Nimbus\Admin\AdminField`

```php
final class Nimbus\Admin\AdminField
```

### Constants

- `TYPES = ["id","text","email","url","password","textarea","number","decimal","boolean","date","datetime","time","select","json","belongsTo"]`

### Properties

- `public ?string $enum`
- `public bool $filterable`
- `public ?Closure $format`
- `public string $help`
- `public string $label`
- `public ?int $maxLength`
- `public readonly string $name`
- `public bool $nullable`
- `public bool $onForm`
- `public bool $onList`
- `public array $options`
- `public bool $readonly`
- `public bool $required`
- `public array $rules`
- `public bool $searchable`
- `public bool $sortable`
- `public ?string $target`
- `public readonly string $type`

### Methods

- `public __construct(string $name, string $type = "text"): mixed`
- `public static belongsTo(string $name, string $target): self`
- `public static boolean(string $name): self`
- `public static date(string $name): self`
- `public static datetime(string $name): self`
- `public static decimal(string $name): self`
- `public static email(string $name): self`
- `public filterable(bool $filterable = true): self`
- `public format(Closure $format): self`
- `public static headline(string $name): string`
- `public help(string $help): self`
- `public hidden(): self`
- `public label(string $label): self`
- `public maxLength(?int $length): self`
- `public nullable(bool $nullable = true): self`
- `public static number(string $name): self`
- `public onForm(bool $show = true): self`
- `public onList(bool $show = true): self`
- `public options(array|string $options): self`
- `public readonly(bool $readonly = true): self`
- `public required(bool $required = true): self`
- `public rules(Nimbus\Validation\ValidationRule ...$rules): self`
- `public searchable(bool $searchable = true): self`
- `public static select(string $name, array|string $options): self`
- `public sortable(bool $sortable = true): self`
- `public static text(string $name): self`
- `public static textarea(string $name): self`

## `Nimbus\Admin\AdminPanel`

```php
final class Nimbus\Admin\AdminPanel
```

### Properties

- `public readonly string $prefix`

### Methods

- `public __construct(Nimbus\App $app, string $prefix = "/admin", ?string $loginUrl = null, string $title = "Admin", ?string $viewPath = null): mixed`
- `public displayTitle(): string`
- `public locale(string $locale): self`
- `public login(): ?string`
- `public loginUrl(?string $url): self`
- `public messages(array $messages): self`
- `public resource(Nimbus\Admin\AdminResource|string $resource): self`
- `public resourceBySlug(string $slug): ?Nimbus\Admin\AdminResource`
- `public resourceFor(string $class): Nimbus\Admin\AdminResource`
- `public resources(): array`
- `public text(string $key, array $replace = []): string`
- `public title(string $title): self`
- `public url(string $path = ""): string`
- `public views(): Nimbus\View\ViewFactory`

## `Nimbus\Admin\AdminResource`

```php
class Nimbus\Admin\AdminResource
```

### Methods

- `public __construct(?string $entity = null): mixed`
- `public actions(): array`
- `public defaultSort(): ?array`
- `public entity(): string`
- `public fields(array $fields): array`
- `public input(): ?string`
- `public label(): string`
- `public perPage(): int`
- `public query(Arcano\ORM\Select $select, Pam\Http\Request $request): Arcano\ORM\Select`
- `public saving(object $entity, bool $creating): void`
- `public singularLabel(): string`
- `public slug(): string`
- `public title(object $entity): string`

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
- `public gate(): Nimbus\Auth\Gate`
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

## `Nimbus\Auth\Abilities`

```php
final readonly class Nimbus\Auth\Abilities
```

### Methods

- `public static allows(array $granted, string $ability): bool`
- `public static normalize(array $abilities): array`

## `Nimbus\Auth\AccessTokenIssuer`

```php
abstract interface Nimbus\Auth\AccessTokenIssuer
```

### Methods

- `public issue(string $subject, array $abilities, int $ttlSeconds = 900): string`

## `Nimbus\Auth\AccessTokenVerifier`

```php
abstract interface Nimbus\Auth\AccessTokenVerifier
```

### Methods

- `public verify(string $token): ?Nimbus\Auth\TokenPrincipal`

## `Nimbus\Auth\AuthContext`

```php
final readonly class Nimbus\Auth\AuthContext
```

### Properties

- `public readonly Nimbus\Auth\Principal $principal`

### Methods

- `public __construct(Nimbus\Auth\Principal $principal): mixed`

## `Nimbus\Auth\AuthRoutes`

```php
final readonly class Nimbus\Auth\AuthRoutes
```

### Methods

- `public __construct(?Nimbus\Auth\PasswordResets $passwords = null, ?Nimbus\Auth\EmailVerification $verification = null, ?Nimbus\Auth\TokenPairIssuer $tokens = null, ?Nimbus\Auth\OAuth\SocialLogin $social = null, ?Nimbus\RateLimit\RateLimitStore $throttleStore = null, int $throttleBurst = 10, Nimbus\Http\ClientIpResolver $clientIp = {"@object":"Nimbus\\Http\\ClientIpResolver","properties":[]}): mixed`
- `public forgotPassword(Pam\Http\Request $request, Pam\Http\Response $response): Pam\Http\Response`
- `public refresh(Pam\Http\Request $request, Pam\Http\Response $response): Pam\Http\Response`
- `public register(Nimbus\App $app, string $path = "/auth"): void`
- `public resendVerification(Pam\Http\Request $request, Pam\Http\Response $response, Nimbus\Auth\Principal $principal): Pam\Http\Response`
- `public resetPassword(Pam\Http\Request $request, Pam\Http\Response $response): Pam\Http\Response`
- `public revoke(Pam\Http\Request $request, Pam\Http\Response $response): Pam\Http\Response`
- `public verifyEmail(Pam\Http\Request $request, Pam\Http\Response $response): Pam\Http\Response`

## `Nimbus\Auth\AuthTablesMigration`

```php
final class Nimbus\Auth\AuthTablesMigration extends Arcano\Migrations\Migration implements Arcano\Migrations\MigrationInterface
```

### Methods

- `public down(): void`
- `public up(): void`

## `Nimbus\Auth\AuthUser`

```php
abstract interface Nimbus\Auth\AuthUser implements Nimbus\Notifications\Notifiable
```

### Methods

- `public authIdentifier(): string`
- `public hasVerifiedEmail(): bool`

## `Nimbus\Auth\AuthUserProvider`

```php
abstract interface Nimbus\Auth\AuthUserProvider
```

### Methods

- `public findByEmail(string $email): ?Nimbus\Auth\AuthUser`
- `public findById(string $identifier): ?Nimbus\Auth\AuthUser`
- `public markEmailAsVerified(Nimbus\Auth\AuthUser $user): void`
- `public updatePassword(Nimbus\Auth\AuthUser $user, string $passwordHash): void`

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

- `public __construct(Nimbus\Auth\AccessTokenVerifier $tokens, ?Nimbus\Auth\TokenRevocationStore $revocations = null, ?callable $clock = null): mixed`
- `public authenticate(Pam\Http\Request $request): ?Nimbus\Auth\Principal`

## `Nimbus\Auth\ChainAuthenticator`

```php
final readonly class Nimbus\Auth\ChainAuthenticator implements Nimbus\Auth\Authenticator
```

### Methods

- `public __construct(Nimbus\Auth\Authenticator ...$authenticators): mixed`
- `public authenticate(Pam\Http\Request $request): ?Nimbus\Auth\Principal`

## `Nimbus\Auth\DatabaseTokenRevocationStore`

```php
final readonly class Nimbus\Auth\DatabaseTokenRevocationStore implements Nimbus\Auth\TokenRevocationStore
```

### Methods

- `public __construct(Arcano\Database\DatabaseInterface|Nimbus\Database\ArcanoDatabaseProvider $database, ?string $connection = null, ?Closure $clock = null): mixed`
- `public isRevoked(string $tokenIdentifier, int $now): bool`
- `public prune(): int`
- `public revoke(string $tokenIdentifier, int $expiresAt): void`

## `Nimbus\Auth\EmailVerification`

```php
final readonly class Nimbus\Auth\EmailVerification
```

### Constants

- `PURPOSE = "email-verification"`

### Methods

- `public __construct(Nimbus\Auth\AuthUserProvider $users, Nimbus\Auth\OneTimeTokens $tokens, Nimbus\Notifications\NotificationDispatcher $notifications, string $verifyUrl, int $ttlSeconds = 86400, int $throttleSeconds = 60, ?Closure $notification = null): mixed`
- `public send(Nimbus\Auth\AuthUser $user): bool`
- `public users(): Nimbus\Auth\AuthUserProvider`
- `public verify(string $token): bool`

## `Nimbus\Auth\Gate`

```php
final class Nimbus\Auth\Gate
```

### Methods

- `public __construct(Nimbus\Container\Container $container): mixed`
- `public allows(string $ability, mixed ...$arguments): bool`
- `public authorize(string $ability, mixed ...$arguments): void`
- `public before(Closure $callback): self`
- `public check(?Nimbus\Auth\Principal $principal, string $ability, array $arguments = []): bool`
- `public define(string $ability, Closure $callback): self`
- `public denies(string $ability, mixed ...$arguments): bool`
- `public entityForParameter(string $parameter): ?string`
- `public has(string $ability): bool`
- `public hasBeforeHooks(): bool`
- `public policy(string $entity, string $policy): self`
- `public policyFor(object|string $entity): ?object`
- `public principal(): ?Nimbus\Auth\Principal`
- `public wildcardScopesGrantUndefinedAbilities(bool $grant = true): self`

## `Nimbus\Auth\HmacTokenCodec`

```php
final readonly class Nimbus\Auth\HmacTokenCodec implements Nimbus\Auth\AccessTokenIssuer, Nimbus\Auth\AccessTokenVerifier
```

### Methods

- `public __construct(string $secret, string $issuer, string $audience, int $leewaySeconds = 5, ?callable $clock = null, string $keyIdentifier = "primary", array $verificationKeys = []): mixed`
- `public issue(string $subject, array $abilities, int $ttlSeconds = 900): string`
- `public verify(string $token): ?Nimbus\Auth\TokenPrincipal`

## `Nimbus\Auth\JwtAuthenticator`

```php
final readonly class Nimbus\Auth\JwtAuthenticator implements Nimbus\Auth\Authenticator
```

### Methods

- `public __construct(Nimbus\Auth\JwtCodec $codec, ?Nimbus\Auth\TokenRevocationStore $revocations = null): mixed`
- `public authenticate(Pam\Http\Request $request): ?Nimbus\Auth\Principal`

## `Nimbus\Auth\JwtCodec`

```php
final readonly class Nimbus\Auth\JwtCodec implements Nimbus\Auth\AccessTokenIssuer, Nimbus\Auth\AccessTokenVerifier
```

### Methods

- `public static asymmetric(string $publicKey, ?string $privateKey, string $issuer, string $audience, string $algorithm = "RS256", string $keyIdentifier = "primary"): self`
- `public static fromJwks(array $jwks, string $issuer, string $audience, ?string $defaultAlgorithm = null): self`
- `public static fromKeySet(ArrayAccess $keys, string $issuer, string $audience): self`
- `public static hmac(string $secret, string $issuer, string $audience, string $algorithm = "HS256", string $keyIdentifier = "primary"): self`
- `public issue(string $subject, array $abilities, int $ttlSeconds = 900, array $claims = []): string`
- `public verify(string $token): ?Nimbus\Auth\TokenPrincipal`

## `Nimbus\Auth\LoginTokenIssuer`

```php
abstract interface Nimbus\Auth\LoginTokenIssuer
```

### Methods

- `public issueLoginToken(string $subject, array $abilities = ["*"]): array`

## `Nimbus\Auth\MemoryTokenRevocationStore`

```php
final class Nimbus\Auth\MemoryTokenRevocationStore implements Nimbus\Auth\TokenRevocationStore
```

### Methods

- `public __construct(int $maximumEntries = 10000): mixed`
- `public isRevoked(string $tokenIdentifier, int $now): bool`
- `public revoke(string $tokenIdentifier, int $expiresAt): void`

## `Nimbus\Auth\NewPersonalAccessToken`

```php
final readonly class Nimbus\Auth\NewPersonalAccessToken
```

### Properties

- `public readonly Nimbus\Auth\PersonalAccessToken $accessToken`
- `public readonly string $plainTextToken`

### Methods

- `public __construct(string $plainTextToken, Nimbus\Auth\PersonalAccessToken $accessToken): mixed`
- `public toArray(): array`

## `Nimbus\Auth\NewRefreshToken`

```php
final readonly class Nimbus\Auth\NewRefreshToken
```

### Properties

- `public readonly string $plainTextToken`
- `public readonly Nimbus\Auth\RefreshToken $token`

### Methods

- `public __construct(string $plainTextToken, Nimbus\Auth\RefreshToken $token): mixed`

## `Nimbus\Auth\OAuth\DatabaseOAuthStateStore`

```php
final readonly class Nimbus\Auth\OAuth\DatabaseOAuthStateStore implements Nimbus\Auth\OAuth\OAuthStateStore
```

### Methods

- `public __construct(Arcano\Database\DatabaseInterface|Nimbus\Database\ArcanoDatabaseProvider $database, ?string $connection = null, ?Closure $clock = null): mixed`
- `public prune(): int`
- `public pull(string $state, int $now): ?array`
- `public put(string $state, array $data, int $expiresAt): void`

## `Nimbus\Auth\OAuth\MemoryOAuthStateStore`

```php
final class Nimbus\Auth\OAuth\MemoryOAuthStateStore implements Nimbus\Auth\OAuth\OAuthStateStore
```

### Methods

- `public __construct(int $maximumEntries = 10000): mixed`
- `public pull(string $state, int $now): ?array`
- `public put(string $state, array $data, int $expiresAt): void`

## `Nimbus\Auth\OAuth\OAuthStateStore`

```php
abstract interface Nimbus\Auth\OAuth\OAuthStateStore
```

### Methods

- `public pull(string $state, int $now): ?array`
- `public put(string $state, array $data, int $expiresAt): void`

## `Nimbus\Auth\OAuth\SocialIdentity`

```php
final readonly class Nimbus\Auth\OAuth\SocialIdentity
```

### Properties

- `public readonly ?string $email`
- `public readonly bool $emailVerified`
- `public readonly string $id`
- `public readonly ?string $name`
- `public readonly string $provider`
- `public readonly array $raw`

### Methods

- `public __construct(string $provider, string $id, ?string $email, ?string $name, bool $emailVerified, array $raw): mixed`
- `public static fromResourceOwner(string $provider, League\OAuth2\Client\Provider\ResourceOwnerInterface $owner): self`

## `Nimbus\Auth\OAuth\SocialLogin`

```php
final readonly class Nimbus\Auth\OAuth\SocialLogin
```

### Methods

- `public __construct(array $providers, Nimbus\Auth\OAuth\OAuthStateStore $states, Nimbus\Auth\OAuth\SocialUserResolver $users, Nimbus\Auth\LoginTokenIssuer $tokens, array $abilities = ["*"], int $stateTtlSeconds = 600, ?Closure $clock = null): mixed`
- `public authorizationUrl(string $provider, array $scopes = []): array`
- `public callback(string $provider, string $state, string $code): array`
- `public providers(): array`
- `public routes(Nimbus\App $app, string $path = "/auth/oauth", array $middleware = []): void`

## `Nimbus\Auth\OAuth\SocialUserResolver`

```php
abstract interface Nimbus\Auth\OAuth\SocialUserResolver
```

### Methods

- `public resolve(Nimbus\Auth\OAuth\SocialIdentity $identity): string`

## `Nimbus\Auth\OneTimeTokens`

```php
final readonly class Nimbus\Auth\OneTimeTokens
```

### Methods

- `public __construct(Arcano\Database\DatabaseInterface|Nimbus\Database\ArcanoDatabaseProvider $database, ?string $connection = null, ?Closure $clock = null): mixed`
- `public consume(string $purpose, string $token): ?string`
- `public create(string $purpose, string $subject, int $ttlSeconds): string`
- `public createdAt(string $purpose, string $subject): ?int`
- `public createdWithin(string $purpose, string $subject, int $seconds): bool`
- `public invalidate(string $purpose, string $subject): int`
- `public prune(): int`

## `Nimbus\Auth\PasswordResets`

```php
final readonly class Nimbus\Auth\PasswordResets
```

### Constants

- `PURPOSE = "password-reset"`

### Methods

- `public __construct(Nimbus\Auth\AuthUserProvider $users, Nimbus\Auth\OneTimeTokens $tokens, Nimbus\Notifications\NotificationDispatcher $notifications, string $resetUrl, int $ttlSeconds = 3600, int $throttleSeconds = 60, int $minimumPasswordLength = 8, ?Closure $notification = null, ?Closure $afterReset = null, ?array $revoke = null): mixed`
- `public assertPassword(string $password): void`
- `public reset(string $token, string $password): bool`
- `public sendResetLink(string $email): bool`

## `Nimbus\Auth\PersonalAccessToken`

```php
final readonly class Nimbus\Auth\PersonalAccessToken
```

### Properties

- `public readonly array $abilities`
- `public readonly int $createdAt`
- `public readonly ?int $expiresAt`
- `public readonly string $id`
- `public readonly ?int $lastUsedAt`
- `public readonly string $name`
- `public readonly string $subject`

### Methods

- `public __construct(string $id, string $subject, string $name, array $abilities, ?int $lastUsedAt, ?int $expiresAt, int $createdAt): mixed`
- `public can(string $ability): bool`
- `public expired(int $now): bool`
- `public toArray(): array`

## `Nimbus\Auth\PersonalAccessTokenAuthenticator`

```php
final readonly class Nimbus\Auth\PersonalAccessTokenAuthenticator implements Nimbus\Auth\Authenticator
```

### Methods

- `public __construct(Nimbus\Auth\PersonalAccessTokens $tokens): mixed`
- `public authenticate(Pam\Http\Request $request): ?Nimbus\Auth\Principal`

## `Nimbus\Auth\PersonalAccessTokens`

```php
final readonly class Nimbus\Auth\PersonalAccessTokens implements Nimbus\Auth\RevokesCredentials
```

### Methods

- `public __construct(Arcano\Database\DatabaseInterface|Nimbus\Database\ArcanoDatabaseProvider $database, string $prefix = "nbp_", int $lastUsedResolution = 60, ?string $connection = null, ?Closure $clock = null): mixed`
- `public create(string $subject, string $name, array $abilities = ["*"], ?int $expiresAt = null): Nimbus\Auth\NewPersonalAccessToken`
- `public find(string $plainTextToken): ?Nimbus\Auth\PersonalAccessToken`
- `public prune(): int`
- `public revoke(string $id, ?string $subject = null): bool`
- `public revokeAll(string $subject): int`
- `public tokensFor(string $subject): array`
- `public use(string $plainTextToken): ?Nimbus\Auth\PersonalAccessToken`

## `Nimbus\Auth\Principal`

```php
abstract interface Nimbus\Auth\Principal
```

### Methods

- `public can(string $ability): bool`
- `public identifier(): string`

## `Nimbus\Auth\RefreshToken`

```php
final readonly class Nimbus\Auth\RefreshToken
```

### Properties

- `public readonly array $abilities`
- `public readonly int $expiresAt`
- `public readonly string $familyId`
- `public readonly string $id`
- `public readonly string $subject`

### Methods

- `public __construct(string $id, string $familyId, string $subject, array $abilities, int $expiresAt): mixed`

## `Nimbus\Auth\RefreshTokenReusedException`

```php
final class Nimbus\Auth\RefreshTokenReusedException extends RuntimeException implements Stringable, Throwable
```

### Properties

- `public readonly string $familyId`
- `public readonly string $subject`

### Methods

- `public __construct(string $familyId, string $subject): mixed`

## `Nimbus\Auth\RefreshTokens`

```php
final readonly class Nimbus\Auth\RefreshTokens implements Nimbus\Auth\RevokesCredentials
```

### Methods

- `public __construct(Arcano\Database\DatabaseInterface|Nimbus\Database\ArcanoDatabaseProvider $database, int $ttlSeconds = 2592000, string $prefix = "nbr_", ?string $connection = null, ?Closure $clock = null): mixed`
- `public issue(string $subject, array $abilities = ["*"]): Nimbus\Auth\NewRefreshToken`
- `public prune(int $graceSeconds = 86400): int`
- `public revoke(string $plainTextToken): bool`
- `public revokeAll(string $subject): int`
- `public revokeFamily(string $familyId): int`
- `public rotate(string $plainTextToken): ?Nimbus\Auth\NewRefreshToken`

## `Nimbus\Auth\ResetPasswordNotification`

```php
final class Nimbus\Auth\ResetPasswordNotification extends Nimbus\Notifications\Notification
```

### Properties

- `public readonly int $expiresInMinutes`
- `public readonly string $url`

### Methods

- `public __construct(string $url, int $expiresInMinutes): mixed`
- `public toMail(Nimbus\Notifications\Notifiable $notifiable): Nimbus\Notifications\Mail\MailMessage`
- `public via(Nimbus\Notifications\Notifiable $notifiable): array`

## `Nimbus\Auth\RevokesCredentials`

```php
abstract interface Nimbus\Auth\RevokesCredentials
```

### Methods

- `public revokeAll(string $subject): int`

## `Nimbus\Auth\SessionAuthenticator`

```php
final readonly class Nimbus\Auth\SessionAuthenticator implements Nimbus\Auth\Authenticator
```

### Methods

- `public __construct(Nimbus\Container\Container $container, Closure $resolver, ?Nimbus\Auth\SessionRevocations $revocations = null, ?Closure $clock = null): mixed`
- `public authenticate(Pam\Http\Request $request): ?Nimbus\Auth\Principal`
- `public login(Nimbus\Auth\Principal $principal): void`
- `public logout(): void`

## `Nimbus\Auth\SessionRevocations`

```php
final readonly class Nimbus\Auth\SessionRevocations implements Nimbus\Auth\RevokesCredentials
```

### Constants

- `PURPOSE = "session-revocation"`

### Methods

- `public __construct(Nimbus\Auth\OneTimeTokens $tokens, int $ttlSeconds = 604800): mixed`
- `public revokeAll(string $subject): int`
- `public revokedAt(string $subject): ?int`

## `Nimbus\Auth\TokenPair`

```php
final readonly class Nimbus\Auth\TokenPair
```

### Properties

- `public readonly array $abilities`
- `public readonly string $accessToken`
- `public readonly int $expiresIn`
- `public readonly int $refreshExpiresAt`
- `public readonly string $refreshToken`

### Methods

- `public __construct(string $accessToken, int $expiresIn, string $refreshToken, int $refreshExpiresAt, array $abilities): mixed`
- `public toArray(): array`

## `Nimbus\Auth\TokenPairIssuer`

```php
final readonly class Nimbus\Auth\TokenPairIssuer implements Nimbus\Auth\LoginTokenIssuer
```

### Methods

- `public __construct(Nimbus\Auth\AccessTokenIssuer $accessTokens, Nimbus\Auth\RefreshTokens $refreshTokens, int $accessTtlSeconds = 900): mixed`
- `public issue(string $subject, array $abilities = ["*"]): Nimbus\Auth\TokenPair`
- `public issueLoginToken(string $subject, array $abilities = ["*"]): array`
- `public refresh(string $refreshToken): Nimbus\Auth\TokenPair`
- `public revoke(string $refreshToken): bool`

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

## `Nimbus\Auth\VerifyEmailNotification`

```php
final class Nimbus\Auth\VerifyEmailNotification extends Nimbus\Notifications\Notification
```

### Properties

- `public readonly int $expiresInMinutes`
- `public readonly string $url`

### Methods

- `public __construct(string $url, int $expiresInMinutes): mixed`
- `public toMail(Nimbus\Notifications\Notifiable $notifiable): Nimbus\Notifications\Mail\MailMessage`
- `public via(Nimbus\Notifications\Notifiable $notifiable): array`

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
- `public beginScope(array $entries = []): void`
- `public bind(string $id, callable|null|string $factory = null): self`
- `public bindRoute(string $class, callable $resolver): self`
- `public call(array $callable, array $named = [], array $provided = []): mixed`
- `public callFunction(Closure $function, array $named = [], array $provided = []): mixed`
- `public diagnostics(): array`
- `public endScope(): void`
- `public get(string $id): mixed`
- `public has(string $id): bool`
- `public instance(string $id, mixed $instance): self`
- `public resolveRouteBinding(string $class, string $value): object`
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

## `Nimbus\Http\Cookie`

```php
final readonly class Nimbus\Http\Cookie
```

### Properties

- `public readonly ?string $domain`
- `public readonly bool $httpOnly`
- `public readonly int $maxAge`
- `public readonly string $name`
- `public readonly string $path`
- `public readonly string $sameSite`
- `public readonly bool $secure`
- `public readonly string $value`

### Methods

- `public __construct(string $name, string $value, int $maxAge = 0, string $path = "/", ?string $domain = null, bool $secure = true, bool $httpOnly = true, string $sameSite = "Lax"): mixed`
- `public static forget(string $name, string $path = "/", ?string $domain = null): self`
- `public toHeader(?int $now = null): string`
- `public withValue(string $value): self`

## `Nimbus\Http\CookieCodec`

```php
final readonly class Nimbus\Http\CookieCodec
```

### Methods

- `public __construct(Nimbus\Security\Crypt $crypt, string $key): mixed`
- `public decrypt(string $name, string $payload): ?string`
- `public encrypt(string $name, string $value): string`
- `public static fromEnvironment(): self`
- `public sign(string $name, string $value): string`
- `public verify(string $name, string $payload): ?string`

## `Nimbus\Http\CookieJar`

```php
final class Nimbus\Http\CookieJar
```

### Methods

- `public forget(string $name, string $path = "/", ?string $domain = null): self`
- `public make(string $name, string $value, int $maxAge = 0): self`
- `public queue(Nimbus\Http\Cookie $cookie): self`
- `public queued(): array`

## `Nimbus\Http\FormData`

```php
final class Nimbus\Http\FormData
```

### Methods

- `public static field(Pam\Http\Request $request, string $name): ?string`
- `public static parse(Pam\Http\Request $request): array`

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
- `CsrfTokenMismatch = 13`

### Constants

- `Conflict = 5`
- `CsrfTokenMismatch = 13`
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

## `Nimbus\Http\Redirect`

```php
final readonly class Nimbus\Http\Redirect implements Nimbus\Http\Responsable
```

### Properties

- `public readonly string $location`
- `public readonly int $status`

### Methods

- `public __construct(string $location, int $status = 302): mixed`
- `public static back(Pam\Http\Request $request, string $fallback = "/", int $status = 302): self`
- `public static to(string $location, int $status = 302): self`
- `public toResponse(Pam\Http\Request $request, Pam\Http\Response $response): Pam\Http\Response`

## `Nimbus\Http\RequestCookies`

```php
final readonly class Nimbus\Http\RequestCookies
```

### Methods

- `public __construct(array $cookies = []): mixed`
- `public all(): array`
- `public static fromRequest(Pam\Http\Request $request): self`
- `public get(string $name, ?string $default = null): ?string`
- `public has(string $name): bool`
- `public static parse(string $header): array`

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

- `public __construct(Nimbus\Container\Container $container, string $ability, ?string $subject = null): mixed`
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

## `Nimbus\Middleware\EncryptCookiesMiddleware`

```php
final readonly class Nimbus\Middleware\EncryptCookiesMiddleware implements Pam\Contracts\Http\MiddlewareInterface
```

### Methods

- `public __construct(Nimbus\Container\Container $container, Nimbus\Http\CookieCodec $codec, array $except = ["nimbus_session","XSRF-TOKEN"], array $signed = []): mixed`
- `public process(Pam\Http\Request $request, Pam\Http\Response $response, Pam\Contracts\Http\RequestHandlerInterface $next): Pam\Http\Response`

## `Nimbus\Middleware\IdempotencyMiddleware`

```php
final readonly class Nimbus\Middleware\IdempotencyMiddleware implements Pam\Contracts\Http\MiddlewareInterface
```

### Methods

- `public __construct(Nimbus\Idempotency\IdempotencyStore $store, int $ttlSeconds = 86400, bool $required = true): mixed`
- `public process(Pam\Http\Request $request, Pam\Http\Response $response, Pam\Contracts\Http\RequestHandlerInterface $next): Pam\Http\Response`

## `Nimbus\Middleware\MethodOverrideMiddleware`

```php
final readonly class Nimbus\Middleware\MethodOverrideMiddleware implements Pam\Contracts\Http\MiddlewareInterface
```

### Methods

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

## `Nimbus\Middleware\StartSessionMiddleware`

```php
final readonly class Nimbus\Middleware\StartSessionMiddleware implements Pam\Contracts\Http\MiddlewareInterface
```

### Methods

- `public __construct(Nimbus\Container\Container $container, Nimbus\Session\SessionStore $store, Nimbus\Session\SessionConfig $config = {"@object":"Nimbus\\Session\\SessionConfig","properties":{"cookie":"nimbus_session","domain":null,"expireOnClose":false,"lifetime":7200,"lottery":[2,100],"path":"/","sameSite":"Lax","secure":true}}, bool $redirectValidationErrors = true): mixed`
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

## `Nimbus\Middleware\VerifyCsrfTokenMiddleware`

```php
final readonly class Nimbus\Middleware\VerifyCsrfTokenMiddleware implements Pam\Contracts\Http\MiddlewareInterface
```

### Constants

- `BEARER_ONLY = "nimbus.csrf.bearer_only"`

### Methods

- `public __construct(Nimbus\Container\Container $container, array $except = [], bool $xsrfCookie = true, bool $secureCookie = true): mixed`
- `public process(Pam\Http\Request $request, Pam\Http\Response $response, Pam\Contracts\Http\RequestHandlerInterface $next): Pam\Http\Response`

## `Nimbus\Notifications\Database\DatabaseChannel`

```php
final readonly class Nimbus\Notifications\Database\DatabaseChannel implements Nimbus\Notifications\NotificationChannel
```

### Methods

- `public __construct(Nimbus\Database\ArcanoDatabaseProvider $databases, string $table = "notifications", ?string $connection = null): mixed`
- `public deliver(array $payload): void`
- `public prepare(Nimbus\Notifications\Notifiable $notifiable, Nimbus\Notifications\Notification $notification, string $id): ?array`

## `Nimbus\Notifications\Database\DatabaseNotification`

```php
final readonly class Nimbus\Notifications\Database\DatabaseNotification implements JsonSerializable
```

### Properties

- `public readonly DateTimeImmutable $createdAt`
- `public readonly array $data`
- `public readonly string $id`
- `public readonly ?DateTimeImmutable $readAt`
- `public readonly string $type`

### Methods

- `public __construct(string $id, string $type, array $data, ?DateTimeImmutable $readAt, DateTimeImmutable $createdAt): mixed`
- `public jsonSerialize(): array`
- `public read(): bool`

## `Nimbus\Notifications\Database\DatabaseNotifications`

```php
final readonly class Nimbus\Notifications\Database\DatabaseNotifications
```

### Methods

- `public __construct(Nimbus\Database\ArcanoDatabaseProvider $databases, string $table = "notifications", ?string $connection = null): mixed`
- `public delete(Nimbus\Notifications\Notifiable $notifiable, string $id): bool`
- `public find(Nimbus\Notifications\Notifiable $notifiable, string $id): ?Nimbus\Notifications\Database\DatabaseNotification`
- `public latest(Nimbus\Notifications\Notifiable $notifiable, int $limit = 50, bool $unreadOnly = false, ?string $before = null): array`
- `public markAllAsRead(Nimbus\Notifications\Notifiable $notifiable): int`
- `public markAsRead(Nimbus\Notifications\Notifiable $notifiable, string $id): bool`
- `public markAsUnread(Nimbus\Notifications\Notifiable $notifiable, string $id): bool`
- `public prune(DateTimeInterface $before): int`
- `public unreadCount(Nimbus\Notifications\Notifiable $notifiable): int`

## `Nimbus\Notifications\Http\CurlHttpClient`

```php
final class Nimbus\Notifications\Http\CurlHttpClient implements Nimbus\Notifications\Http\HttpClient
```

### Methods

- `public __construct(): mixed`
- `public send(Nimbus\Notifications\Http\HttpRequest $request): Nimbus\Notifications\Http\HttpResponse`

## `Nimbus\Notifications\Http\HttpClient`

```php
abstract interface Nimbus\Notifications\Http\HttpClient
```

### Methods

- `public send(Nimbus\Notifications\Http\HttpRequest $request): Nimbus\Notifications\Http\HttpResponse`

## `Nimbus\Notifications\Http\HttpClients`

```php
final class Nimbus\Notifications\Http\HttpClients
```

### Methods

- `public static detect(): Nimbus\Notifications\Http\HttpClient`

## `Nimbus\Notifications\Http\HttpRequest`

```php
final readonly class Nimbus\Notifications\Http\HttpRequest
```

### Properties

- `public readonly string $body`
- `public readonly array $headers`
- `public readonly bool $http2`
- `public readonly string $method`
- `public readonly float $timeout`
- `public readonly string $url`

### Methods

- `public __construct(string $method, string $url, array $headers = [], string $body = "", float $timeout = 10, bool $http2 = false): mixed`

## `Nimbus\Notifications\Http\HttpResponse`

```php
final readonly class Nimbus\Notifications\Http\HttpResponse
```

### Properties

- `public readonly string $body`
- `public readonly array $headers`
- `public readonly int $status`

### Methods

- `public __construct(int $status, array $headers = [], string $body = ""): mixed`
- `public header(string $name): ?string`
- `public json(): array`
- `public successful(): bool`

## `Nimbus\Notifications\Http\HttpTransportException`

```php
final class Nimbus\Notifications\Http\HttpTransportException extends RuntimeException implements Stringable, Throwable
```

## `Nimbus\Notifications\Http\PamHttpClient`

```php
final class Nimbus\Notifications\Http\PamHttpClient implements Nimbus\Notifications\Http\HttpClient
```

### Methods

- `public __construct(?Nimbus\Notifications\Http\HttpClient $http2 = null): mixed`
- `public send(Nimbus\Notifications\Http\HttpRequest $request): Nimbus\Notifications\Http\HttpResponse`

## `Nimbus\Notifications\Http\Psr18HttpClient`

```php
final readonly class Nimbus\Notifications\Http\Psr18HttpClient implements Nimbus\Notifications\Http\HttpClient
```

### Methods

- `public __construct(Psr\Http\Client\ClientInterface $client, Psr\Http\Message\RequestFactoryInterface $requests, Psr\Http\Message\StreamFactoryInterface $streams): mixed`
- `public send(Nimbus\Notifications\Http\HttpRequest $request): Nimbus\Notifications\Http\HttpResponse`

## `Nimbus\Notifications\Mail\MailChannel`

```php
final readonly class Nimbus\Notifications\Mail\MailChannel implements Nimbus\Notifications\NotificationChannel
```

### Methods

- `public __construct(Symfony\Component\Mailer\Transport\TransportInterface $transport, string $fromAddress, string $fromName = ""): mixed`
- `public deliver(array $payload): void`
- `public static dsn(array $environment): string`
- `public static fromDsn(string $dsn, string $fromAddress, string $fromName = ""): self`
- `public static fromEnvironment(?array $environment = null): self`
- `public prepare(Nimbus\Notifications\Notifiable $notifiable, Nimbus\Notifications\Notification $notification, string $id): ?array`

## `Nimbus\Notifications\Mail\MailMessage`

```php
final class Nimbus\Notifications\Mail\MailMessage
```

### Methods

- `public action(string $text, string $url): self`
- `public bcc(string $address): self`
- `public cc(string $address): self`
- `public from(string $address, string $name = ""): self`
- `public getSubject(): string`
- `public greeting(string $greeting): self`
- `public html(string $html): self`
- `public line(string $line): self`
- `public static make(): self`
- `public renderHtml(): string`
- `public renderText(): string`
- `public replyTo(string $address): self`
- `public salutation(string $salutation): self`
- `public subject(string $subject): self`
- `public text(string $text): self`
- `public toArray(): array`

## `Nimbus\Notifications\Notifiable`

```php
abstract interface Nimbus\Notifications\Notifiable
```

### Methods

- `public notifiableKey(): string`
- `public notifiableType(): string`
- `public routeNotificationFor(string $channel, Nimbus\Notifications\Notification $notification): mixed`

## `Nimbus\Notifications\Notification`

```php
abstract class Nimbus\Notifications\Notification
```

### Methods

- `public delaySeconds(): int`
- `public maximumAttempts(): int`
- `public toDatabase(Nimbus\Notifications\Notifiable $notifiable): array`
- `public toMail(Nimbus\Notifications\Notifiable $notifiable): Nimbus\Notifications\Mail\MailMessage`
- `public toPush(Nimbus\Notifications\Notifiable $notifiable): Nimbus\Notifications\Push\PushMessage`
- `public toWebhook(Nimbus\Notifications\Notifiable $notifiable): Nimbus\Notifications\Webhook\WebhookMessage|array`
- `public type(): string`
- `public via(Nimbus\Notifications\Notifiable $notifiable): array`

## `Nimbus\Notifications\NotificationChannel`

```php
abstract interface Nimbus\Notifications\NotificationChannel
```

### Methods

- `public deliver(array $payload): void`
- `public prepare(Nimbus\Notifications\Notifiable $notifiable, Nimbus\Notifications\Notification $notification, string $id): ?array`

## `Nimbus\Notifications\NotificationDeliveryException`

```php
final class Nimbus\Notifications\NotificationDeliveryException extends RuntimeException implements Stringable, Throwable
```

### Properties

- `public readonly bool $retryable`

### Methods

- `public __construct(string $message, bool $retryable = true, ?Throwable $previous = null): mixed`
- `public static permanent(string $message, ?Throwable $previous = null): self`

## `Nimbus\Notifications\NotificationDispatcher`

```php
abstract interface Nimbus\Notifications\NotificationDispatcher
```

### Methods

- `public send(Nimbus\Notifications\Notifiable|Traversable|array $notifiables, Nimbus\Notifications\Notification $notification): void`
- `public sendLater(Nimbus\Notifications\Notifiable|Traversable|array $notifiables, Nimbus\Notifications\Notification $notification, ?int $delaySeconds = null): array`

## `Nimbus\Notifications\NotificationFake`

```php
final class Nimbus\Notifications\NotificationFake implements Nimbus\Notifications\NotificationDispatcher
```

### Methods

- `public assertNotSentTo(Nimbus\Notifications\Notifiable $notifiable, string $notification, ?callable $callback = null): void`
- `public assertNothingSent(): void`
- `public assertSentLaterTo(Nimbus\Notifications\Notifiable $notifiable, string $notification): void`
- `public assertSentTimes(string $notification, int $times): void`
- `public assertSentTo(Nimbus\Notifications\Notifiable $notifiable, string $notification, ?callable $callback = null): void`
- `public send(Nimbus\Notifications\Notifiable|Traversable|array $notifiables, Nimbus\Notifications\Notification $notification): void`
- `public sendLater(Nimbus\Notifications\Notifiable|Traversable|array $notifiables, Nimbus\Notifications\Notification $notification, ?int $delaySeconds = null): array`
- `public sent(?string $notification = null): array`

## `Nimbus\Notifications\NotificationSender`

```php
final class Nimbus\Notifications\NotificationSender
```

### Methods

- `public __construct(array $channels = []): mixed`
- `public channel(string $name, Nimbus\Notifications\NotificationChannel $channel): self`
- `public deliver(string $channel, array $payload): void`
- `public get(string $name): Nimbus\Notifications\NotificationChannel`
- `public has(string $name): bool`
- `public prepare(Nimbus\Notifications\Notifiable $notifiable, Nimbus\Notifications\Notification $notification, ?string $id = null): array`
- `public send(Nimbus\Notifications\Notifiable $notifiable, Nimbus\Notifications\Notification $notification): void`
- `public static uuid(): string`

## `Nimbus\Notifications\Notifier`

```php
final readonly class Nimbus\Notifications\Notifier implements Nimbus\Notifications\NotificationDispatcher
```

### Methods

- `public __construct(Nimbus\Notifications\NotificationSender $sender, ?Nimbus\Jobs\JobDispatcher $jobs = null): mixed`
- `public static fake(): Nimbus\Notifications\NotificationFake`
- `public send(Nimbus\Notifications\Notifiable|Traversable|array $notifiables, Nimbus\Notifications\Notification $notification): void`
- `public sendLater(Nimbus\Notifications\Notifiable|Traversable|array $notifiables, Nimbus\Notifications\Notification $notification, ?int $delaySeconds = null): array`

## `Nimbus\Notifications\OnDemandNotifiable`

```php
final class Nimbus\Notifications\OnDemandNotifiable implements Nimbus\Notifications\Notifiable
```

### Methods

- `public and(string $channel, mixed $route): self`
- `public notifiableKey(): string`
- `public notifiableType(): string`
- `public static route(string $channel, mixed $route): self`
- `public routeNotificationFor(string $channel, Nimbus\Notifications\Notification $notification): mixed`

## `Nimbus\Notifications\Push\ApnsProvider`

```php
final class Nimbus\Notifications\Push\ApnsProvider implements Nimbus\Notifications\Push\PushProvider
```

### Methods

- `public __construct(string $teamId, string $keyId, string $privateKey, string $topic, bool $production = true, ?Nimbus\Notifications\Http\HttpClient $http = null, float $timeout = 10, ?Closure $clock = null): mixed`
- `public static fromKeyFile(string $teamId, string $keyId, string $path, string $topic, bool $production = true, ?Nimbus\Notifications\Http\HttpClient $http = null): self`
- `public send(Nimbus\Notifications\Push\PushMessage $message, string $token): Nimbus\Notifications\Push\PushOutcome`

## `Nimbus\Notifications\Push\FcmProvider`

```php
final class Nimbus\Notifications\Push\FcmProvider implements Nimbus\Notifications\Push\PushProvider
```

### Methods

- `public __construct(array $serviceAccount, ?Nimbus\Notifications\Http\HttpClient $http = null, float $timeout = 10, ?Closure $clock = null): mixed`
- `public static fromServiceAccountFile(string $path, ?Nimbus\Notifications\Http\HttpClient $http = null): self`
- `public static fromServiceAccountJson(string $json, ?Nimbus\Notifications\Http\HttpClient $http = null): self`
- `public send(Nimbus\Notifications\Push\PushMessage $message, string $token): Nimbus\Notifications\Push\PushOutcome`

## `Nimbus\Notifications\Push\JsonWebToken`

```php
final class Nimbus\Notifications\Push\JsonWebToken
```

### Methods

- `public static base64Url(string $bytes): string`
- `public static es256(array $claims, string $privateKeyPem, array $header = []): string`
- `public static rs256(array $claims, string $privateKeyPem, array $header = []): string`

## `Nimbus\Notifications\Push\PushChannel`

```php
final readonly class Nimbus\Notifications\Push\PushChannel implements Nimbus\Notifications\NotificationChannel
```

### Methods

- `public __construct(?Nimbus\Notifications\Push\PushProvider $fcm = null, ?Nimbus\Notifications\Push\PushProvider $apns = null, ?Closure $onInvalidToken = null): mixed`
- `public deliver(array $payload): void`
- `public prepare(Nimbus\Notifications\Notifiable $notifiable, Nimbus\Notifications\Notification $notification, string $id): ?array`

## `Nimbus\Notifications\Push\PushMessage`

```php
final class Nimbus\Notifications\Push\PushMessage
```

### Methods

- `public badge(int $badge): self`
- `public body(string $body): self`
- `public collapseKey(string $key): self`
- `public data(array $data): self`
- `public static fromArray(array $data): self`
- `public getCollapseKey(): ?string`
- `public image(string $url): self`
- `public isHighPriority(): bool`
- `public isSilent(): bool`
- `public static make(?string $title = null, ?string $body = null): self`
- `public normalPriority(): self`
- `public silent(): self`
- `public sound(string $sound = "default"): self`
- `public threadId(string $threadId): self`
- `public title(string $title): self`
- `public toApns(): array`
- `public toArray(): array`
- `public toFcm(): array`
- `public ttl(int $seconds): self`
- `public ttlSeconds(): ?int`

## `Nimbus\Notifications\Push\PushOutcome`

```php
final enum Nimbus\Notifications\Push\PushOutcome implements BackedEnum, UnitEnum: int
```

### Enum cases

- `Delivered = 1`
- `InvalidToken = 2`

### Constants

- `Delivered = 1`
- `InvalidToken = 2`

### Properties

- `public readonly string $name`
- `public readonly int $value`

### Methods

- `public static cases(): array`
- `public static from(int|string $value): static`
- `public static tryFrom(int|string $value): ?static`

## `Nimbus\Notifications\Push\PushPlatform`

```php
final enum Nimbus\Notifications\Push\PushPlatform implements BackedEnum, UnitEnum: string
```

### Enum cases

- `Fcm = "fcm"`
- `Apns = "apns"`

### Constants

- `Apns = "apns"`
- `Fcm = "fcm"`

### Properties

- `public readonly string $name`
- `public readonly string $value`

### Methods

- `public static cases(): array`
- `public static from(int|string $value): static`
- `public static tryFrom(int|string $value): ?static`

## `Nimbus\Notifications\Push\PushProvider`

```php
abstract interface Nimbus\Notifications\Push\PushProvider
```

### Methods

- `public send(Nimbus\Notifications\Push\PushMessage $message, string $token): Nimbus\Notifications\Push\PushOutcome`

## `Nimbus\Notifications\Push\PushTarget`

```php
final readonly class Nimbus\Notifications\Push\PushTarget
```

### Properties

- `public readonly Nimbus\Notifications\Push\PushPlatform $platform`
- `public readonly string $token`

### Methods

- `public __construct(Nimbus\Notifications\Push\PushPlatform $platform, string $token): mixed`
- `public static apns(string $token): self`
- `public static fcm(string $token): self`
- `public static parse(string $target): self`
- `public toString(): string`

## `Nimbus\Notifications\SendNotificationHandler`

```php
final readonly class Nimbus\Notifications\SendNotificationHandler implements Nimbus\Jobs\JobHandler
```

### Methods

- `public __construct(Nimbus\Notifications\NotificationSender $sender): mixed`
- `public handle(object $job): Nimbus\Jobs\JobOutcome`

## `Nimbus\Notifications\SendNotificationJob`

```php
final readonly class Nimbus\Notifications\SendNotificationJob implements Nimbus\Jobs\SerializableJob
```

### Properties

- `public readonly string $channel`
- `public readonly array $payload`

### Methods

- `public __construct(string $channel, array $payload): mixed`
- `public static fromJobPayload(array $payload): self`
- `public static jobName(): string`
- `public toJobPayload(): array`

## `Nimbus\Notifications\Webhook\WebhookChannel`

```php
final readonly class Nimbus\Notifications\Webhook\WebhookChannel implements Nimbus\Notifications\NotificationChannel
```

### Methods

- `public __construct(string $secret, ?Nimbus\Notifications\Http\HttpClient $http = null, float $timeout = 10, ?Closure $secretFor = null, ?Closure $clock = null): mixed`
- `public deliver(array $payload): void`
- `public prepare(Nimbus\Notifications\Notifiable $notifiable, Nimbus\Notifications\Notification $notification, string $id): ?array`

## `Nimbus\Notifications\Webhook\WebhookMessage`

```php
final class Nimbus\Notifications\Webhook\WebhookMessage
```

### Methods

- `public __construct(array $data = []): mixed`
- `public data(array $data): self`
- `public event(string $event): self`
- `public header(string $name, string $value): self`
- `public static make(array $data = []): self`
- `public toArray(): array`

## `Nimbus\Notifications\Webhook\WebhookSignature`

```php
final class Nimbus\Notifications\Webhook\WebhookSignature
```

### Constants

- `HEADER = "Nimbus-Signature"`

### Methods

- `public static sign(string $secret, string $body, int $timestamp): string`
- `public static verify(string $header, string $body, string $secret, int $toleranceSeconds = 300, ?int $now = null): bool`

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
- `public can(string $ability, ?string $subject = null): self`
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

## `Nimbus\Redis\RedisOAuthStateStore`

```php
final readonly class Nimbus\Redis\RedisOAuthStateStore implements Nimbus\Auth\OAuth\OAuthStateStore
```

### Methods

- `public __construct(Nimbus\Redis\Connection $redis, string $prefix = "nimbus:oauth:"): mixed`
- `public pull(string $state, int $now): ?array`
- `public put(string $state, array $data, int $expiresAt): void`

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

## `Nimbus\Redis\RedisSessionStore`

```php
final readonly class Nimbus\Redis\RedisSessionStore implements Nimbus\Session\SessionStore
```

### Methods

- `public __construct(Nimbus\Redis\Connection $redis, string $prefix = "nimbus:session:"): mixed`
- `public destroy(string $id): void`
- `public gc(): int`
- `public read(string $id): ?array`
- `public write(string $id, array $data, int $lifetimeSeconds): void`

## `Nimbus\Redis\RedisTokenRevocationStore`

```php
final readonly class Nimbus\Redis\RedisTokenRevocationStore implements Nimbus\Auth\TokenRevocationStore
```

### Methods

- `public __construct(Nimbus\Redis\Connection $redis, string $prefix = "nimbus:revoked:"): mixed`
- `public isRevoked(string $tokenIdentifier, int $now): bool`
- `public revoke(string $tokenIdentifier, int $expiresAt): void`

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
- `public can(string $ability, ?string $subject = null): self`
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

## `Nimbus\Session\DatabaseSessionStore`

```php
final readonly class Nimbus\Session\DatabaseSessionStore implements Nimbus\Session\SessionStore
```

### Methods

- `public __construct(Arcano\Database\DatabaseInterface|Nimbus\Database\ArcanoDatabaseProvider $database, string $table = "sessions", ?string $connection = null, ?Closure $clock = null): mixed`
- `public destroy(string $id): void`
- `public gc(): int`
- `public read(string $id): ?array`
- `public write(string $id, array $data, int $lifetimeSeconds): void`

## `Nimbus\Session\ErrorBag`

```php
final readonly class Nimbus\Session\ErrorBag implements Countable
```

### Methods

- `public __construct(array $messages = []): mixed`
- `public all(): array`
- `public any(): bool`
- `public count(): int`
- `public first(?string $field = null, ?string $default = null): ?string`
- `public get(string $field): array`
- `public has(string $field): bool`
- `public isEmpty(): bool`
- `public toArray(): array`

## `Nimbus\Session\FileSessionStore`

```php
final readonly class Nimbus\Session\FileSessionStore implements Nimbus\Session\SessionStore
```

### Methods

- `public __construct(string $directory, ?Closure $clock = null): mixed`
- `public destroy(string $id): void`
- `public gc(): int`
- `public read(string $id): ?array`
- `public write(string $id, array $data, int $lifetimeSeconds): void`

## `Nimbus\Session\MemorySessionStore`

```php
final class Nimbus\Session\MemorySessionStore implements Nimbus\Session\SessionStore
```

### Methods

- `public __construct(?Closure $clock = null): mixed`
- `public destroy(string $id): void`
- `public gc(): int`
- `public read(string $id): ?array`
- `public write(string $id, array $data, int $lifetimeSeconds): void`

## `Nimbus\Session\Session`

```php
final class Nimbus\Session\Session
```

### Methods

- `public __construct(string $id, array $attributes = [], bool $existed = false): mixed`
- `public ageFlashData(): void`
- `public all(): array`
- `public static current(): ?self`
- `public static enter(?self $session): void`
- `public errors(): Nimbus\Session\ErrorBag`
- `public existed(): bool`
- `public flash(string $key, mixed $value = true): self`
- `public flashErrors(array $errors): self`
- `public flashInput(array $input): self`
- `public flush(): self`
- `public forget(string ...$keys): self`
- `public get(string $key, mixed $default = null): mixed`
- `public has(string $key): bool`
- `public id(): string`
- `public invalidate(): self`
- `public isDirty(): bool`
- `public static newId(): string`
- `public now(string $key, mixed $value): self`
- `public old(?string $key = null, mixed $default = null): mixed`
- `public previousId(): ?string`
- `public pull(string $key, mixed $default = null): mixed`
- `public put(array|string $key, mixed $value = null): self`
- `public reflash(string ...$keys): self`
- `public regenerate(): self`
- `public regenerateToken(): string`
- `public token(): string`

## `Nimbus\Session\SessionConfig`

```php
final readonly class Nimbus\Session\SessionConfig
```

### Properties

- `public readonly string $cookie`
- `public readonly ?string $domain`
- `public readonly bool $expireOnClose`
- `public readonly int $lifetime`
- `public readonly array $lottery`
- `public readonly string $path`
- `public readonly string $sameSite`
- `public readonly bool $secure`

### Methods

- `public __construct(string $cookie = "nimbus_session", int $lifetime = 7200, bool $expireOnClose = false, string $path = "/", ?string $domain = null, bool $secure = true, string $sameSite = "Lax", array $lottery = [2,100]): mixed`

## `Nimbus\Session\SessionServiceProvider`

```php
final readonly class Nimbus\Session\SessionServiceProvider implements Pam\Contracts\Package\ServiceProviderInterface
```

### Methods

- `public __construct(Nimbus\Session\SessionStore $store, Nimbus\Session\SessionConfig $config = {"@object":"Nimbus\\Session\\SessionConfig","properties":{"cookie":"nimbus_session","domain":null,"expireOnClose":false,"lifetime":7200,"lottery":[2,100],"path":"/","sameSite":"Lax","secure":true}}, ?Nimbus\Http\CookieCodec $cookies = null, bool $middleware = true, array $csrfExcept = []): mixed`
- `public boot(Pam\Contracts\Http\ApplicationInterface $application): void`
- `public register(Pam\Contracts\Http\ApplicationInterface $application): void`

## `Nimbus\Session\SessionStore`

```php
abstract interface Nimbus\Session\SessionStore
```

### Methods

- `public destroy(string $id): void`
- `public gc(): int`
- `public read(string $id): ?array`
- `public write(string $id, array $data, int $lifetimeSeconds): void`

## `Nimbus\Session\SessionTablesMigration`

```php
final class Nimbus\Session\SessionTablesMigration extends Arcano\Migrations\Migration implements Arcano\Migrations\MigrationInterface
```

### Methods

- `public down(): void`
- `public up(): void`

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
- `public form(string $method, string $path, array $form = [], array $headers = []): Nimbus\Testing\TestResponse`
- `public get(string $path, array $headers = []): Nimbus\Testing\TestResponse`
- `public getJson(string $path, array $headers = []): Nimbus\Testing\TestResponse`
- `public json(string $method, string $path, ?array $payload = null, array $headers = []): Nimbus\Testing\TestResponse`
- `public patchJson(string $path, array $payload = [], array $headers = []): Nimbus\Testing\TestResponse`
- `public post(string $path, array $form = [], array $headers = []): Nimbus\Testing\TestResponse`
- `public postJson(string $path, array $payload = [], array $headers = []): Nimbus\Testing\TestResponse`
- `public putJson(string $path, array $payload = [], array $headers = []): Nimbus\Testing\TestResponse`
- `public request(string $method, string $path, array $query = [], array $headers = [], string $body = ""): Nimbus\Testing\TestResponse`
- `public withCookies(array $cookies): self`
- `public withCookiesFrom(Nimbus\Testing\TestResponse $response): self`
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
- `public assertDontSee(string $text, bool $escape = true): self`
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
- `public assertRedirect(?string $location = null): self`
- `public assertSee(string $text, bool $escape = true): self`
- `public assertStatus(int $expected): self`
- `public assertSuccessful(): self`
- `public assertUnauthorized(): self`
- `public assertUnprocessableEntity(): self`
- `public body(): string`
- `public cookie(string $name): ?string`
- `public cookies(): array`
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

## `Nimbus\Translation\LangDirectoryLoader`

```php
final readonly class Nimbus\Translation\LangDirectoryLoader implements Symfony\Component\Translation\Loader\LoaderInterface
```

### Methods

- `public load(mixed $resource, string $locale, string $domain = "messages"): Symfony\Component\Translation\MessageCatalogue`
- `public static locales(string $directory): array`

## `Nimbus\Translation\Locale`

```php
final readonly class Nimbus\Translation\Locale
```

### Properties

- `public readonly string $code`

### Methods

- `public __construct(string $code): mixed`
- `public static normalize(string $code): string`

## `Nimbus\Translation\LocaleMiddleware`

```php
final readonly class Nimbus\Translation\LocaleMiddleware implements Pam\Contracts\Http\MiddlewareInterface
```

### Methods

- `public __construct(Nimbus\Translation\Translator $translator, Nimbus\Container\Container $container, ?string $queryParameter = "lang", string $sessionKey = "locale"): mixed`
- `public process(Pam\Http\Request $request, Pam\Http\Response $response, Pam\Contracts\Http\RequestHandlerInterface $next): Pam\Http\Response`

## `Nimbus\Translation\TranslationServiceProvider`

```php
final readonly class Nimbus\Translation\TranslationServiceProvider implements Pam\Contracts\Package\ServiceProviderInterface
```

### Methods

- `public __construct(string $directory, string $defaultLocale = "en", array $fallbackLocales = [], ?array $supportedLocales = null, bool $middleware = true): mixed`
- `public boot(Pam\Contracts\Http\ApplicationInterface $application): void`
- `public register(Pam\Contracts\Http\ApplicationInterface $application): void`

## `Nimbus\Translation\Translator`

```php
final class Nimbus\Translation\Translator
```

### Methods

- `public __construct(string $directory, string $defaultLocale = "en", array $fallbackLocales = [], ?Nimbus\Container\Container $container = null, ?array $supportedLocales = null): mixed`
- `public choice(string $key, float|int $count, array $replace = [], ?string $locale = null): string`
- `public static default(): ?self`
- `public directory(): string`
- `public get(string $key, array $replace = [], ?string $locale = null): string`
- `public has(string $key, ?string $locale = null): bool`
- `public locale(): string`
- `public negotiate(?string $acceptLanguage): ?string`
- `public supportedLocales(): array`
- `public supports(string $locale): bool`
- `public useAsDefault(): self`

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

## `Nimbus\View\AttributeBag`

```php
final readonly class Nimbus\View\AttributeBag implements Countable, IteratorAggregate, Nimbus\View\Htmlable, Stringable, Traversable
```

### Methods

- `public __construct(array $attributes = []): mixed`
- `public __toString(): string`
- `public class(array $classes): self`
- `public static classList(array $classes): string`
- `public count(): int`
- `public except(array|string $names): self`
- `public get(string $name, mixed $default = null): mixed`
- `public getAttributes(): array`
- `public getIterator(): ArrayIterator`
- `public has(string $name): bool`
- `public isEmpty(): bool`
- `public merge(array $defaults): self`
- `public only(array|string $names): self`
- `public toHtml(): string`
- `public whereStartsWith(string $prefix): self`

## `Nimbus\View\CompiledTemplate`

```php
final readonly class Nimbus\View\CompiledTemplate
```

### Properties

- `public readonly string $code`
- `public readonly array $references`

### Methods

- `public __construct(string $code, array $references = []): mixed`

## `Nimbus\View\Compiler`

```php
final class Nimbus\View\Compiler
```

### Methods

- `public static compile(string $source, string $template = "inline"): Nimbus\View\CompiledTemplate`

## `Nimbus\View\Component`

```php
abstract class Nimbus\View\Component
```

### Properties

- `public Nimbus\View\AttributeBag $attributes`

### Methods

- `public render(): Nimbus\View\Htmlable|Nimbus\View\View|string`
- `public shouldRender(): bool`
- `public withAttributes(Nimbus\View\AttributeBag $attributes): static`

## `Nimbus\View\HtmlString`

```php
readonly class Nimbus\View\HtmlString implements Nimbus\View\Htmlable, Stringable
```

### Methods

- `public __construct(string $html = ""): mixed`
- `public __toString(): string`
- `public isEmpty(): bool`
- `public isNotEmpty(): bool`
- `public toHtml(): string`

## `Nimbus\View\Htmlable`

```php
abstract interface Nimbus\View\Htmlable
```

### Methods

- `public toHtml(): string`

## `Nimbus\View\Loop`

```php
final class Nimbus\View\Loop
```

### Properties

- `public readonly ?int $count`
- `public readonly int $depth`
- `public bool $even`
- `public bool $first`
- `public int $index`
- `public int $iteration`
- `public bool $last`
- `public bool $odd`
- `public readonly ?Nimbus\View\Loop $parent`
- `public ?int $remaining`

### Methods

- `public __construct(?int $count, int $depth, ?Nimbus\View\Loop $parent): mixed`
- `public advance(): void`

## `Nimbus\View\Slot`

```php
final readonly class Nimbus\View\Slot extends Nimbus\View\HtmlString implements Nimbus\View\Htmlable, Stringable
```

### Properties

- `public readonly Nimbus\View\AttributeBag $attributes`

### Methods

- `public __construct(string $html = "", ?Nimbus\View\AttributeBag $attributes = null): mixed`

## `Nimbus\View\View`

```php
final readonly class Nimbus\View\View implements Nimbus\Http\Responsable, Nimbus\View\Htmlable, Stringable
```

### Properties

- `public readonly array $data`
- `public readonly string $name`
- `public readonly int $status`

### Methods

- `public __construct(Nimbus\View\ViewFactory $factory, string $name, array $data = [], int $status = 200): mixed`
- `public __toString(): string`
- `public render(): string`
- `public toHtml(): string`
- `public toResponse(Pam\Http\Request $request, Pam\Http\Response $response): Pam\Http\Response`
- `public with(array|string $key, mixed $value = null): self`
- `public withStatus(int $status): self`

## `Nimbus\View\ViewChecker`

```php
final readonly class Nimbus\View\ViewChecker
```

### Methods

- `public __construct(Nimbus\View\ViewFactory $factory): mixed`
- `public check(): array`

## `Nimbus\View\ViewException`

```php
final class Nimbus\View\ViewException extends RuntimeException implements Stringable, Throwable
```

### Properties

- `public readonly string $template`
- `public readonly int $templateLine`

### Methods

- `public __construct(string $message, string $template, int $templateLine, ?Throwable $previous = null): mixed`
- `public reason(): string`

## `Nimbus\View\ViewFactory`

```php
final class Nimbus\View\ViewFactory
```

### Constants

- `EXTENSION = ".nim.php"`

### Methods

- `public __construct(array|string $paths, ?Nimbus\Container\Container $container = null, string $componentNamespace = "App\\Views", bool $reload = false, ?Nimbus\Translation\Translator $translator = null): mixed`
- `public addNamespace(string $namespace, array|string $paths): self`
- `public compiled(string $view): array`
- `public component(string $name): array`
- `public container(): ?Nimbus\Container\Container`
- `public static default(): self`
- `public static evaluate(string $code, string $template): Closure`
- `public exists(string $view): bool`
- `public make(string $view, array $data = []): Nimbus\View\View`
- `public path(string $view): string`
- `public render(string $view, array $data = []): string`
- `public renderComponent(Nimbus\View\Renderer $renderer, string $name, array $attributes, array $slots): string`
- `public renderString(string $source, array $data = []): string`
- `public setTranslator(?Nimbus\Translation\Translator $translator): self`
- `public share(string $key, mixed $value): self`
- `public shared(): array`
- `public templates(): array`
- `public translator(): ?Nimbus\Translation\Translator`
- `public useAsDefault(): self`

## `Nimbus\View\ViewServiceProvider`

```php
final readonly class Nimbus\View\ViewServiceProvider implements Pam\Contracts\Package\ServiceProviderInterface
```

### Methods

- `public __construct(array|string $paths, string $componentNamespace = "App\\Views", bool $reload = false): mixed`
- `public boot(Pam\Contracts\Http\ApplicationInterface $application): void`
- `public register(Pam\Contracts\Http\ApplicationInterface $application): void`
