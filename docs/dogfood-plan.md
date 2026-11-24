# Nimbus dogfood plan: buck-log-intelligence

Pilot of Nimbus on a production Pushin service before 1.0. Nothing here is
deployed; the service repository was only read. The port and the benchmark live
outside both repositories, in `nimbus-bench/dogfood/buck-log-intelligence`
(notebook workspace), with `README.md`, the Nimbus app, the seed, the wrk
scripts and the raw results.

## Chosen service and why

`buck-log-intelligence` (Laravel 13.17, Octane, Postgres, Redis cache) is the
API behind Buck, the PushinPay sales-analytics assistant.

| Candidate | Laravel | Routes | Controllers | Models | Excluded because |
| --- | --- | ---: | ---: | ---: | --- |
| **buck-log-intelligence** | 13.17 | 29 | 11 | 14 | chosen |
| nf-backend | 12 | 13 | 6 | 10 | almost only writes to an external invoice API |
| pushinfy-backend | 12 | 49 | 19 | 20 | Telegram, Firebase, RabbitMQ, Mailgun, 2FA integrations |
| chat-pushin-backend | 13.8 | 186 | 23 | 72 | no own git history, no vendor |
| api-backend-v2 | 12.70 | 176 | 163 | 164 | Inertia, activitylog, dompdf, Twilio, Reverb |
| pushinpay-* | 11/12 | 186-442 | 64-137 | 94-210 | size, payment flows |

Route counts are `Route::<verb>` matches in the route files.

- 29 API routes, 18 of them `GET`; 4 `POST`, 2 `PUT`, 1 `PATCH`, 3 `DELETE`.
- Dependencies: `laravel/framework`, `laravel/octane`, `laravel/tinker`,
  `firebase/php-jwt`. No Sanctum, events, broadcasting, mail, notifications,
  Horizon or spatie packages.
- Auth is two small middlewares: an HS256 account JWT issued by PushinPay
  (`BuckApiToken`) and a static internal bearer (`BuckInternalToken`).
- Read paths are simple indexed queries on Postgres; heavier endpoints use
  Postgres-specific SQL (`percentile_cont`, `count(*) filter`, `date_trunc`,
  partitions), a good later test for Arcano's raw query path.

## Endpoints to port, in order

All paths are under `/api/buck`. "JWT" means `BuckApiToken` plus `throttle:buck`
(120 per minute).

| # | Method and path | Laravel action | Reads | Auth |
| ---: | --- | --- | --- | --- |
| 1 | `GET subscriptions/{account}` | `SubscriptionsController@show` | accounts, subscriptions, subscription_changes | internal token |
| 2 | `GET accounts/{a}/signals` | `BuckController@signals` | last 50 signals | JWT |
| 3 | `GET accounts/{a}/reports` | `BuckController@reports` | last 12 reports | JWT |
| 4 | `GET accounts/{a}/opportunities` | `BuckController@opportunities` | opportunities, `?status`, limit 100 | JWT |
| 5 | `GET accounts/{a}/events` | `BuckController@events` | log_events (partitioned), `?kind_id`, `?limit` <= 200 | JWT |
| 6 | `GET accounts/{a}/series` | `BuckController@series` | daily aggregates | JWT |
| 7 | `GET accounts/{a}/charges` | `ChargesController@index` | charges + dictionaries, `before_id` paging | JWT |
| 8 | `GET accounts/{a}/charges/stats` | `ChargesController@stats` | charge_stats_day + today with percentiles | JWT |
| 9 | `GET accounts/{a}/overview` | `BuckController@overview` | about 8 queries | JWT |
| 10 | `GET subscriptions?status=&limit=` | `SubscriptionsController@index` | subscriptions join accounts | internal token |
| 11 | `GET security/summary`, `security/events`, `security/ip/{ip}` | `SecurityController` | security_events, GROUP BY | admin token |
| 12 | `GET health` | `HealthController` | pipeline snapshot, 503 when degraded | admin token |
| 13 | `PUT accounts/{a}/goal` | `BuckController@goal` | `updateOrCreate` on goals | JWT |
| 14 | `PATCH accounts/{a}/opportunities/{id}` | `BuckController@updateOpportunity` | single update | JWT |

Last: `stream` (SSE), `ask`, `ask/{id}/action`, `ask/defense`, `codex/test`
(LLM calls with `Cache::lock`) and `subscriptions/sync` (batch write).

Endpoints 1 to 3 are already ported (below).

## Ported endpoints and local benchmark

Endpoints 1-3 run on Nimbus (branch `feat/nimbus-1.0-docs`, Arcano 0.2.8) with
the same middleware order and checks: uuid format (404), token (401), `sub`
(403), account (402), cached active subscription (402), `AccountMonitor`
created on first access.

Parity: `parity.py` sends 39 requests to both apps (success, unknown and
invalid uuid, missing/expired/foreign/badly signed tokens, `exp` missing,
wrong `aud`/`iss`, expired and canceled subscriptions, monitor creation) and
compares status, content type and body bytes. 39 of 39 are identical without
normalization.

Benchmark on the notebook (Intel Core Ultra 9 185H), 2026-11-23:

- Laravel: unmodified service code, Octane on FrankenPHP 1.13.6 (PHP 8.5.13),
  `artisan optimize`, opcache on, JIT off.
- Nimbus: `pam start` (PAM 2.2.25, PHP 8.5.12).
- Both: 4 workers pinned to CPUs 0-3, one app at a time; Postgres 17.12 limited
  to 2 CPUs and 1 GB; wrk on CPUs 8-9, `-t2 -c32`, 3 s warm-up, 10 s measured,
  median of 3 alternating runs; 1000 accounts, 100 benchmarked accounts with
  200 signals, 24 reports and 20 subscription changes each; 0 non-2xx responses.
  The machine was in use (1-minute load 2.9 to 6.6).

| Endpoint | App | req/s | p50 ms | p99 ms |
| --- | --- | ---: | ---: | ---: |
| `subscriptions/{account}` | Laravel | 1,721 | 18.50 | 22.17 |
| `subscriptions/{account}` | Nimbus | 8,363 | 3.69 | 7.30 |
| `accounts/{a}/signals` | Laravel | 390 | 81.83 | 98.44 |
| `accounts/{a}/signals` | Nimbus | 1,667 | 18.53 | 37.08 |
| `accounts/{a}/reports` | Laravel | 1,150 | 27.63 | 32.82 |
| `accounts/{a}/reports` | Nimbus | 6,023 | 5.07 | 10.33 |
| `signals`, extra index `(account_id, id)` | Laravel | 508 | 62.29 | 73.43 |
| `signals`, extra index `(account_id, id)` | Nimbus | 4,168 | 7.47 | 14.52 |

Nimbus/Laravel throughput: 4.86x (subscriptions), 4.27x (signals), 5.24x
(reports), 8.21x (signals with the index). RSS after the last run: Laravel
355 MB (including the 134 MB artisan parent), Nimbus 218 MB (76 MB PSS).

On `signals` with the production schema Postgres is the bottleneck (2 of 2
CPUs busy, Nimbus at about 1.7 of 4): without an `(account_id, id)` index the
last-50 query walks the primary key backwards and discards about 19,800 rows
for a busy account. That index is worth adding to the Laravel service
regardless of Nimbus.

## What Nimbus was missing for this service

Closed in Nimbus `Unreleased` (see `packages/nimbus/CHANGELOG.md`): the pilot can drop its
encoder, error middleware, typecasts, `CurrentAccount` wrapper, `bin/cache-schema` and path
repository workarounds, and port the commands, schedule and write endpoints as written.

| Gap | Impact on the pilot | Issue |
| --- | --- | --- |
| `Response::json()` has fixed flags | byte parity needed a custom encoder | #213 |
| errors are always problem+json; `onError()` does not cover `HttpException` | middleware returns responses instead of throwing; router 404/405 still differ | #214 |
| no database timezone setting; naive timestamps read as UTC | callable typecast per date column | #215 |
| rate limit is per second only, no named limiters | `throttle:buck` mapped to 2 rps with burst 120; `throttle:3,1` impossible | #216 |
| no `firstOrCreate`/`updateOrCreate` | select + persist, racy under concurrency | #217 |
| no `Cache::lock()` | blocks `ask`, heartbeat lock | #218 |
| no request attributes | `CurrentAccount` scoped wrapper | #219 |
| schema cache build needs a custom script | `bin/cache-schema` in the pilot | #220 |
| no app console commands or `Schedule::command()` | about 20 commands and 13 schedule entries stay on Laravel | #221 |
| `FormRequest` rule coverage (`date`, `between`, pipe strings, `messages()`) | blocks `goal`, `events`, `series` validation as written | #222 |
| Arcano dist ships tests; path repo copies Nimbus `vendor/` | 221 MB app vendor in the pilot | #223 |
| `pushinbr/nimbus-rector` unpublished | install through a path repository | #224 |

No Nimbus or Arcano bug showed up in the ported endpoints.

## Rollout plan

The Laravel service stays the system of record during the whole pilot; Nimbus
only answers `GET` endpoints until the write gaps (#217, #222) are closed.

1. **Port and verify offline** (done for 1-3). For each endpoint: parity script
   over recorded production requests (status, headers that clients read, body
   bytes), the service's own feature tests ported to `Nimbus\Testing\TestCase`.
2. **Deploy side by side.** The Nimbus app runs on the same hosts with
   `pam deploy:remote`, reading the same Postgres with a read-only database role
   and its own Redis prefix. No public route points to it.
3. **Shadow traffic, 1 week.** The edge (nginx `mirror` or the PAM proxy) copies
   every `GET` of the ported endpoints to Nimbus and discards the answer. A
   comparer logs status, latency and a hash of the body for both sides.
   Exit criteria: body mismatch below 0.1% and all explained, p99 not worse
   than Laravel, no worker recycle caused by memory, error rate equal.
4. **Canary by percentage.** Route a share of real traffic to Nimbus by
   account hash (stable per account, so one user sees one backend):
   1% for 24 h, 5% for 48 h, 25% for 3 days, 50%, then 100%. At each step
   compare per-endpoint error rate, p50/p99 and Postgres load between the two
   cohorts; any regression or a client-visible mismatch rolls the share back
   to 0% with a config change (no deploy).
5. **Writes.** After #217 and #222, repeat steps 3-4 for `PUT goal` and
   `PATCH opportunities/{id}` with shadow writes disabled: canary only, starting
   at 1%, with the Laravel endpoint kept deployed for rollback.
6. **Decommission** the Laravel routes of an endpoint after 2 weeks at 100%
   without rollback. Commands and the scheduler move last (#221).

Owners: the Buck team approves each step; the PAM team fixes the gaps listed
above before the step that needs them.
