# Changelog

All notable changes to Nimbus are documented here. Nimbus follows Semantic
Versioning; the 0.x line may still adjust public API between minor versions.

## Unreleased

Second round of fixes from the 2026-11-20 security audit.

### Security

- `AuthRoutes` throttles through a PAM shared table by default (the new
  `SharedTableRateLimitStore`), so every worker spends one budget and recycling a worker no longer
  resets it. Outside PAM it falls back to a per-process store and warns once in production (M4).
- `POST /auth/password/forgot` answers the same 202 whether or not the address exists, swallows
  transport errors and pads the response to `forgotPasswordMinimumMilliseconds` (250 ms). Unknown
  addresses do the same lookup and hashing work. `PasswordResets(queue: ...)` moves the whole
  lookup-and-send to a `SendPasswordResetLink` job that carries only the address (M5).
- **Breaking:** social login binds the OAuth state to the client that started it: the start route
  sets an HttpOnly, SameSite=Lax `nimbus_oauth_state` cookie and the callback refuses a state
  without it (login CSRF). `bindStateToClient: false` turns it off (M6).
- **Breaking:** e-mail verification tokens are bound to the address they were sent to; a link sent
  before the user changed the address no longer verifies the new one. Links sent before this
  release stop working; users request a new one (M7).
- Storage downloads carry `nosniff`, a sandboxing CSP and `Content-Disposition: attachment`,
  except passive types (`StorageRoutes::DEFAULT_INLINE_TYPES`). Uploads of HTML, SVG, XML or script
  types and extensions are refused with 415 unless `allowActiveContent: true`, and the declared
  `Content-Length` is checked before the body is read (M8).
- `ViewFactory::renderString()` keeps compiled sources in an LRU of 256 entries and documents that
  the source must be trusted (M9).
- `SecurityHeadersMiddleware` also sets its headers on error responses, honours
  `X-Forwarded-Proto` from `trustedProxies` (or with `trustForwardedProto: true`) for HSTS, and
  takes an optional `contentSecurityPolicy` (`DEFAULT_CSP`) that does not override a route's own (B5).
- `Redirect::back()` falls back when the referer path starts with `//` or `/\` (B6).
- `IdempotencyMiddleware` scopes keys by the authenticated principal (with the container) or the
  `Authorization` header (B7).
- Refresh token families have an absolute lifetime, `familyLifetimeSeconds` (30 days by default,
  `null` for none); families issued before this release keep rotating without it (B7).
- CSRF exceptions match whole path segments: `/webhooks*` matches `/webhooks` and `/webhooks/...`,
  no longer `/webhooksX` (B7).
- `Crypt` (and `CookieCodec::fromEnvironment()`) decrypt with `APP_PREVIOUS_KEYS`, so `APP_KEY`
  can be rotated without invalidating encrypted values (B7).
Productivity parity with Laravel. Every addition is optional: the production dependency set is
still `php` and `pushinbr/pam-contracts`.

### Added: gaps found porting `buck-log-intelligence` (#213 to #223)

- `Http\JsonEncoding` and `App::jsonEncoding()`: the `json_encode()` flags of every JSON body
  Nimbus writes (`JsonEncoding::LARAVEL` for the bytes of Laravel's `JsonResponse`;
  `JSON_UNESCAPED_UNICODE` stays the default). Actions may return arrays or `JsonSerializable`,
  answered as JSON with the status already set on the response (#213).
- `App::renderErrorsWith(Http\ErrorRenderer)`: one renderer for every `HttpException`, the router
  404/405, validation 422 and the 500 of unhandled exceptions without `onError()`;
  `ProblemDetailsRenderer` (default) and `LaravelErrorRenderer` (`{"message"}`, `{"message",
  "errors"}`, `Server Error`). The renderer is bound as `ErrorRenderer::class` (#214).
- `timezone` connection option, from `DB_TIMEZONE` or `APP_TIMEZONE` in
  `DatabaseConfig::fromEnvironment()`: the pgsql session `TimeZone` (set at connect), the mysql
  `time_zone`, and the zone the Arcano `datetime` typecast reads naive timestamps in (#215).
- `RateLimit\RateLimiter` and `RateLimit\Limit`: named limiters (`for('buck', fn ($request) =>
  Limit::perMinute(120)->by($id))`), fixed windows per second/minute/hour/day, `attempt()`,
  `hit()`, `tooManyAttempts()`, `availableIn()`, `clear()`; `ThrottleRequestsMiddleware` with
  `PendingRoute::rateLimit('name')`, `throttle(3, 1)`, the `throttle:name` and `throttle:3,1`
  aliases, the same on `RouteRegistrar`, and Laravel's `X-RateLimit-Limit`/`-Remaining`,
  `Retry-After` and `X-RateLimit-Reset` headers. Counters live in the bound cache repository (#216).
- `ArcanoManager::firstOrCreate()`, `updateOrCreate()` and `createOrFirst()`: race-safe under a
  unique index (the losing insert fails inside a savepoint and the winner's row is returned) (#217).
- `Cache::lock()`/`Repository::lock()` and `restoreLock()`: `Cache\Lock` with `get(fn)`,
  `block($seconds, fn)` (`LockTimeoutException`), `release()`, `forceRelease()` and owner tokens;
  `RedisLock` (`SET NX EX`, Lua compare-and-delete) for `RedisCacheStore`, `CacheLock` over `add()`
  for other stores. `Scheduling\CacheSchedulerLock` schedules over the same locks and
  `RedisSchedulerLock` now uses `RedisLock` (#218).
- `Http\RequestContext`: request-scoped values from middleware to actions (`put($account)` typed,
  `set('key', $value)` keyed). An instance in the request scope wins over a route segment of the
  same name when resolving an action parameter (#219).
- `bin/nimbus` on `symfony/console` (suggested): `migrate`, `migrate:status`, `migrate:rollback`,
  `db:seed`, `schema:cache`, `schema:clear`, `schedule:list` and `tinker` over
  `bootstrap/app.php` (#220); application commands as `Console\Command` with Laravel
  `$signature`/`handle()` injection, discovered in `app/Console/Commands` or registered with
  `App::commands()`; `Console\Kernel::call()`; `Schedule::command('name --flag')` (#221).
- `Validation\Validator` behind `FormRequest`: pipe-string rules, `nullable`, `sometimes`, `bail`,
  `required_if`/`_unless`/`_with`/`_without`, `numeric`, `date`, `date_format`, `after`/`before`,
  `min`/`max`/`between`/`size`, `in`/`not_in`, `regex`, `confirmed`, `url`, `uuid` and more,
  `exists`/`unique` over Arcano (`ArcanoPresenceVerifier`, `Rule::exists()`, `Rule::unique()->ignore()`),
  `Rule::in()`, dot paths and `items.*` wildcards, `messages()` and `attributes()`, with Laravel's
  English messages. The PHPStan extension reads pipe rules and `nullable` (#222).
- `packages/nimbus/.gitattributes` and `archive.exclude`: a Composer path repository with
  `symlink: false` no longer copies `vendor/`, `.build/` or `tests/` into the app (#223).

### Changed

- `FormRequest` validates the query string together with the body (the body wins), as Laravel;
  `integer`, `numeric` and `boolean` accept string values and `validated()` returns them cast.
  Non-implicit rules skip missing fields and empty strings, as in Laravel.
- `RateLimitMiddleware` stays the per-second token bucket; use `throttle()` for Laravel windows.

### Added

- `Nimbus\Http\Client`: a fluent HTTP client (`Http::withToken()->timeout()->retry()->get()`,
  `asForm()`, `withBody()`, `baseUrl()`, `pool()`, `Response::throw()`/`json('a.b')`). Inside PAM
  it sends through `Pam\Http\Client`, suspending only the request Fiber, and pools run in sibling
  Fibers; elsewhere it uses `symfony/http-client` (suggested). Retry waits use `Pam\Async\delay()`.
- Test fakes with Laravel-style assertions, bound into the test's application by the
  `Nimbus\Testing` facades and `TestCase::fake*()` helpers: `Queue::fake()` (`Jobs\QueueFake`,
  `assertPushed`), `Event::fake()` (`Events\EventFake`, `assertDispatched`), `Mail::fake()`
  (`Mail\MailFake`, `assertSent`/`assertSentTo`), `Storage::fake()` (temporary local disk with
  `Disk::assertExists()`/`assertMissing()`), `Broadcast::fake()` and `Http::fake()` (URL pattern
  stubs, `Http::sequence()`, `assertSent`, `preventStrayRequests()`).
- `Mail\Mailer`: sends a `MailMessage` or a Symfony `Email` outside notifications
  (`$mailer->to($address)->send(...)`); `channel()` builds the notification mail channel over it.
- Queued event listeners: listeners implementing `Events\ShouldQueue` are pushed to the bound
  `JobDispatcher` as `CallQueuedListener` jobs (`$tries`, `$delay`, `shouldQueue()`).
- Broadcasting: events implementing `Broadcasting\ShouldBroadcast` (or `ShouldBroadcastNow`) go
  to public, private and presence channels through `BroadcastManager`, queued as `BroadcastEvent`
  jobs when a dispatcher is bound. Drivers for PAM's native WebSocket rooms
  (`PamSocketBroadcaster`), the Pusher protocol (`PusherBroadcaster` over
  `pusher/pusher-php-server`: Pusher, Soketi, Reverb) and Redis pub/sub (`RedisBroadcaster`).
  `BroadcastManager::channel()` authorizes subscriptions and `routes()` registers
  `POST /broadcasting/auth`.
- Cache: `Cache\Repository` (`get`, `put`, `add`, `remember`, `increment`, `pull`, `flush`) over
  `MemoryStore` or `Redis\RedisCacheStore`, with tags on every store
  (`Cache::tags(['products'])->put()`/`flush()`) and a static `Cache` facade.
- `bin/nimbus tinker`: a PsySH REPL (`psy/psysh`, suggested) with the booted application from
  `bootstrap/app.php` and `$app`, `$container`, `$arcano`, `$orm` in scope.
- `Profiler\DebugPanel`: a development panel recording requests, queries, exceptions, jobs, mails
  and outgoing HTTP calls per worker, at `/_debug`. Disabled unless `APP_ENV`/`PAM_ENV` is a
  development environment, and guarded by an `Authenticator` and the `viewDebugPanel` ability.
  `MemoryProfiler::currentToken()` exposes the token of the request being profiled.
- PHPStan extension (`phpstan/extension.neon`, registered for `phpstan/extension-installer`):
  `Container::get(Foo::class)` is a `Foo`, `ArcanoManager::repository()`/`select()` are generic
  over the entity, FormRequest `input()`/`validated()` are typed from `rules()`, and a rule checks
  route closure parameters against the path placeholders. `composer analyse` (level 9, in CI)
  covers the extension.
- `Http\RequestData`: Laravel-style input reads for ported code (`input()` with dot notation,
  `all()`, `has()`, `filled()`, `only()`, `bearerToken()`); body fields win over the query
  string. The `pushinbr/nimbus-rector` set rewrites `$request->input()` and friends to it.
- `Deprecation::trigger()` (internal): deprecation notices in the `symfony/deprecation-contracts`
  format, through `trigger_deprecation()` when installed (now suggested).
- `docs/VERSIONING.md`: semver scope, public API inventory, one-minor deprecation cycle and
  1.x LTS (security fixes for 18 months after 2.0); `UPGRADE.md` gains a template for majors.
- CI: `scripts/nimbus-bc-check.sh` runs `roave/backward-compatibility-check` against the newest
  tag of `push-in/nimbus` on every pull request that changes the package.

### Changed

- Marked `@internal` and removed from `api-surface.json` (none is documented or reachable from
  a public signature): `CallableRequestHandler`, `ContainerMiddleware`, `ControllerHandler`,
  `HandlerResolver`, `Pipeline`, `PackageDiscovery`, `View\Compiler`, `View\CompiledTemplate`,
  `Validation\DtoHydrator`. The public surface is 333 symbols.

## 0.6.0 - 2026-11-21

Security hardening; requires `pushinbr/arcano` `^0.2.8`.

### Security

- `ClientIpResolver` takes the client from the right of `X-Forwarded-For`: the rightmost entry
  that is not a trusted proxy, instead of the leftmost entry the client writes itself. This closes
  a bypass of the auth route throttle by a forged header. Trusted proxies accept CIDR ranges;
  an entry that is neither an address nor a range throws at construction.
- **Breaking:** the bare `*` token scope no longer satisfies a Gate ability that has neither a
  definition nor a policy. Default tokens (`PersonalAccessTokens`, `RefreshTokens`, token pairs,
  social login) carry `*`, so any signed-in user passed undefined abilities such as `viewAdmin`.
  Explicit and namespace scopes (`products:write`, `products:*`) still do.
  `$app->gate()->wildcardScopesGrantUndefinedAbilities()` restores the old behaviour.
- **Breaking:** the admin panel answers 403 until the application defines `viewAdmin`
  (`$app->gate()->define('viewAdmin', ...)`) or registers a `before()` hook; a principal's own
  `can('viewAdmin')` is no longer enough.
- `ResponseCacheMiddleware` keys the default cache entry by the request's credentials
  (`Authorization` and `Cookie`) as well, so a cached response of one user is never served to
  another, and never stores responses that set a cookie, are `Cache-Control: private`, `no-store`
  or `no-cache`, or `Vary: *`.
- `VerifyCsrfTokenMiddleware` still skips requests with a bearer token, but such a request can
  then only be authenticated by the token: `SessionAuthenticator` ignores its session cookie. A
  cross-site request with an invented `Authorization: Bearer x` and the victim's cookie no longer
  acts as the victim. If the session already authenticated the request, the CSRF check runs.
- **Breaking:** `CorsMiddleware` throws when `'*'` is combined with `credentials: true`, which
  reflected any origin with credentials.
- **Breaking:** `PasswordResets` signs the user out everywhere on reset through the new `revoke:`
  list of `RevokesCredentials` (`RefreshTokens`, `PersonalAccessTokens` and the new
  `SessionRevocations`); constructing it with neither `revoke:` nor `afterReset:` throws
  (`revoke: []` opts out). Whoever had taken over the account used to stay signed in after the
  victim reset the password. `SessionAuthenticator` accepts a `SessionRevocations` and rejects
  sessions that logged in before the user's last revocation.

## 0.5.2 - 2026-11-19

### Changed

- Each route builds its middleware pipeline once per worker instead of on every request.

## 0.5.1 - 2026-11-19

### Changed

- `Paginator::paginate()` reads the page and the total with one statement when Arcano offers
  `Select::fetchPage()` (0.2.6+): the count is a scalar subquery column of the page query, so a
  numbered page costs one database round trip instead of two. The total is still a real `COUNT`;
  a page past the last row counts with its own statement.
- `ResourceCollection` builds its `data` list without a closure per item.
- Development and tests use `pushinbr/arcano` `^0.2.6` (page and count in one statement, cached insert plans, UTC hydration through `pam_native_utc_datetime()` on PAM).

## 0.5.0 - 2026-11-19

### Added

- Admin panel over Arcano (`Nimbus\Admin`): `Admin::panel($app, '/admin')` registers a dashboard and CRUD
  routes for `AdminResource` classes (or plain entity classes). Default `AdminField`s come from the
  compiled schema and `#[Column]` attributes (types, size, nullability, belongs-to selects) plus the Nimbus
  validation attributes on the entity; resources relabel, hide or add fields, scope the query, validate
  through an `Input` class and declare bulk `AdminAction`s. Search, filters, sorting, pagination, flash
  messages, `viewAdmin` and per-entity policy checks, CSRF, `en`/`pt_BR` texts, light/dark CSS without
  JavaScript dependencies; templates can be overridden by directory.
- CLI: `pam make:admin Product` creates `app/Admin/ProductAdmin.php`.

### Changed

- `ArcanoManager::select()` reuses a select prototype per role and connection (roles with scopes excluded); queries are only timed and logged while a query budget is active.
- Development and tests use `pushinbr/arcano` `^0.2.5` (compiled hydration, `COUNT(*)`, single-statement writes without BEGIN/COMMIT, cached `fetchOne`).

## 0.4.0 - 2026-11-19

### Added

- API authentication (`Nimbus\Auth`): `PersonalAccessTokens` (hashed, scoped, expiring, `last_used_at`,
  revoke one or all) with `PersonalAccessTokenAuthenticator`; rotating `RefreshTokens` with reuse detection
  that revokes the family, and `TokenPairIssuer` / `TokenPair` (OAuth 2 token response); `ChainAuthenticator`.
- `DatabaseTokenRevocationStore` and `Nimbus\Redis\RedisTokenRevocationStore` beside the memory store.
- `JwtCodec` and `JwtAuthenticator` on `firebase/php-jwt` (optional): HS256/384/512, RS256, ES256, EdDSA,
  JWKS and `CachedKeySet` key sets, abilities in the `scope` claim.
- Social login (`Nimbus\Auth\OAuth\SocialLogin`) on `league/oauth2-client` (optional): authorization URL
  with server-side state and PKCE, code exchange, an application `SocialUserResolver` and any
  `LoginTokenIssuer`; `MemoryOAuthStateStore`, `DatabaseOAuthStateStore`, `Nimbus\Redis\RedisOAuthStateStore`.
- Password reset and e-mail verification by API (`PasswordResets`, `EmailVerification`, `OneTimeTokens`,
  `AuthUser`, `AuthUserProvider`) with single-use hashed tokens mailed through Notifications, and
  `AuthRoutes` registering the reset, verification, refresh and social routes with per-IP throttling.
- `Gate` (`App::gate()`): `define()`, `policy()` resolved through the container, `before()` hooks,
  `allows()`, `denies()`, `authorize()` (401/403 problems); `can('update', 'product')` on routes and
  route groups authorizes the bound entity, and abilities without rules fall back to token abilities.
- `AuthTablesMigration`, `Abilities` (wildcards such as `products:*`), `Container::resolveRouteBinding()`.
- Web sessions (`Nimbus\Session`): `Session` (get/put/forget/pull, `flash()`, `now()`, `reflash()`,
  `flashInput()`/`old()`, `flashErrors()`/`errors()` as an `ErrorBag`, CSRF `token()`, `regenerate()`,
  `invalidate()`) on `MemorySessionStore`, `FileSessionStore`, `DatabaseSessionStore`
  (`SessionTablesMigration`) and `Nimbus\Redis\RedisSessionStore`; JSON payloads only, never
  `unserialize()`. `StartSessionMiddleware` keeps the session in the request scope (and
  `Session::current()` per fiber), writes it only when it existed or changed, prunes by lottery and turns
  a `ValidationException` on an HTML request into a redirect back with errors and old input.
- `VerifyCsrfTokenMiddleware`: 419 (`ProblemCode::CsrfTokenMismatch`) unless `_token`, `X-CSRF-TOKEN` or
  `X-XSRF-TOKEN` matches the session token; skips GET/HEAD/OPTIONS, bearer requests and `except` paths;
  sends the `XSRF-TOKEN` cookie.
- Cookies: `Cookie` (Set-Cookie with secure, HttpOnly and SameSite=Lax defaults), `RequestCookies`,
  the scoped `CookieJar`, `CookieCodec` (encrypt with `Crypt` or HMAC-sign, bound to the cookie name)
  and `EncryptCookiesMiddleware`.
- `SessionServiceProvider` (store, method override, cookies, session and CSRF middleware in order),
  `SessionAuthenticator` (web login that regenerates the session id, logout that invalidates it),
  `MethodOverrideMiddleware` (`_method` / `X-HTTP-Method-Override`), `Redirect` (`to()`, same-origin
  `back()`) and `FormData` (urlencoded, multipart text fields, JSON).
- `Nimbus\View`: a Blade-style template engine of its own for `*.nim.php` (`{{ }}` escaped, `{!! !!}`,
  `@if/@elseif/@else`, `@unless`, `@isset`, `@empty`, `@foreach` with `$loop`, `@forelse`, `@for`,
  `@while`, `@continue`/`@break`, `@auth`/`@guest`, `@can`/`@cannot`, `@csrf`, `@method`, `@error`,
  `@include`/`@includeIf`/`@includeWhen`, `@push`/`@prepend`/`@stack`, `@php`, `@extends`/`@section`/
  `@yield`/`@show`/`@parent`, `@props`, `@json`, `@class`, `@checked`/`@selected`/`@disabled`,
  `@lang`, `@verbatim`). Anonymous components in `components/` with `$attributes->merge()`, default and
  named slots (`<x-slot:footer>`), class components (`Component`, `App\Views` by default). Templates
  compile once per worker into closures that append to a string (no output buffering, fiber-safe), keep
  their source lines, and errors are `ViewException` with file and line.
- `ViewFactory`, `View` (a `Responsable`), `ViewServiceProvider`, `ViewChecker` and the
  `vendor/bin/nimbus-views check` command for CI; `view()` helper.
- Translation on `symfony/translation` (optional): `Translator` over `lang/{locale}/*.php` and
  `lang/{locale}.json` (`LangDirectoryLoader`), `:name`/`{name}`/`%name%` placeholders, plurals,
  fallback locales, `Accept-Language` negotiation; `LocaleMiddleware` sets the request's `Locale` from
  `?lang=`, the session or `Accept-Language`; `TranslationServiceProvider`; `__()`, `trans()`,
  `trans_choice()`, `old()` and `csrf_token()` helpers.
- `TestClient::post()`/`form()` (HTML forms), `withCookies()`, `withCookiesFrom()`;
  `TestResponse::cookies()`, `cookie()`, `assertSee()`, `assertDontSee()`, `assertRedirect()`.

### Changed

- `BearerTokenAuthenticator` accepts any `AccessTokenVerifier`; `HmacTokenCodec` implements it and
  `AccessTokenIssuer`.
- `TokenPrincipal::can()` honours namespace wildcards (`orders:*`).
- `Input` and `FormRequest` read urlencoded and multipart form bodies as well as JSON.
- `ArcanoManager::select()` clones one prototype per role and connection set, so repeated queries share
  the loader and Arcano's cached statement of their shape (roles with a scope still build a fresh select).
- The query monitor hooks the Arcano query log only when the first query budget starts; apps without
  `QueryBudgetMiddleware` no longer time and log every query.

## 0.3.0 - 2026-11-19

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
- `Nimbus\Testing\TestCase`: a PHPUnit base that boots a fresh application per test, with
  `getJson`/`postJson`/`putJson`/`patchJson`/`deleteJson`, `withHeaders`, `withToken`, `actingAs`
  (through the `Authenticator` contract), `seed()` and `assertDatabaseHas`/`Missing`/`Count`.
- `RefreshDatabase`: pending migrations, then one transaction per test shared by its requests and
  rolled back at the end.
- `TestResponse`: `assertOk`, `assertCreated`, `assertNoContent`, `assertNotFound`,
  `assertUnauthorized`, `assertForbidden`, `assertUnprocessableEntity`, `assertExactJson`,
  `assertJsonCount`, `assertJsonStructure`, `assertJsonMissingPath`, `assertJsonValidationErrors`,
  `assertJsonMissingValidationErrors`, `assertHeaderMissing`, `json($path)`, `dump()`;
  `assertJsonPath()` accepts a closure.
- `TestClient`: `getJson`, `putJson`, `patchJson`, `deleteJson`, `json()`, `withHeaders`,
  `withHeader`, `withToken`, `actingAs`; query strings in paths are parsed.
- `Nimbus\Database\Factory` (Faker-backed entity factories with `count`, `state`, `sequence`,
  `make`, `create`, `raw` and callbacks), `Seeder` and `SeederRunner`. `fakerphp/faker` is suggested.
- `Container::has()`.

### Changed
- `TestResponse::assertJson()` checks a subset of the payload; use `assertExactJson()` for the
  previous exact match.
- `TestResponse` failures are PHPUnit assertion failures when PHPUnit is loaded (they were
  `RuntimeException`, which remains the fallback).
- `TestClient::postJson()` payload is optional and sends `accept: application/json`.
- Notifications (`Nimbus\Notifications`): `Notification`, `Notifiable` with the `RoutesNotifications`
  trait, on-demand recipients, the `NotificationSender` channel registry and `Notifier::send()` /
  `sendLater()`, which queues one `SendNotificationJob` per recipient and channel through the
  existing `JobDispatcher`; `SendNotificationHandler` retries transient failures and dead-letters
  permanent ones.
- Mail channel on `symfony/mailer` (optional) with the `MailMessage` builder and
  `MAILER_DSN`/`MAIL_MAILER` configuration.
- Push channel: FCM HTTP v1 (service account RS256 assertion, access token cached per worker) and
  APNs (ES256 provider token, HTTP/2), with invalid-token callbacks.
- Database channel on Arcano with the `notifications` migration and `DatabaseNotifications`
  (list, unread count, mark read/unread, delete, prune).
- Webhook channel: JSON POST signed with `Nimbus-Signature: t=..,v1=..` (HMAC-SHA256) and
  `WebhookSignature::verify()` for receivers.
- `HttpClient` transport for push and webhooks: PAM runtime client, curl (HTTP/2) or any PSR-18 client.
- `Notifier::fake()` with `assertSentTo`, `assertNotSentTo`, `assertSentLaterTo`, `assertSentTimes`
  and `assertNothingSent`.

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
