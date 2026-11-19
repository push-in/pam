# Changelog

All notable changes to Nimbus are documented here. Nimbus follows Semantic
Versioning; the 0.x line may still adjust public API between minor versions.

## Unreleased

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
