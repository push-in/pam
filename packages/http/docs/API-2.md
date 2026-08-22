# PAM API 2 design and delivery contract

PAM API 2 has an independent release line. It does not require the native PAM
runtime or unrelated Composer packages to adopt major version 2. A release is
cut from a commit integrated into the monorepo `main` branch, verified as an
isolated split, and published as `v2.x.y` in `push-in/pam-http` with provenance
retained by GitHub Actions.

PAM API combines Express-style route ergonomics with Laravel-style application
boundaries. Small applications may use closures. Product applications can use
controllers, request objects, services, repositories and resources without
changing runtimes.

## Design rules

1. Public APIs are explicit, typed and friendly to static analysis.
2. Reflection is compiled or cached before serving production traffic.
3. Request-specific values use request scope and are never retained by workers.
4. Controllers orchestrate; services own use cases; repositories own persistence.
5. Input uses Form Requests/DTOs and domain output uses Resources.
6. Status/type/state/kind/category codes are sequential integer-backed enums.
7. Every unbounded operation requires limits, cancellation and observability.
8. Optional integrations depend on contracts so the HTTP core stays small.

## Handler forms

All handler forms resolve through the same pipeline:

```php
$app->get('/health', static fn (Request $request, Response $response) =>
    $response->json(['status' => 1]));

$app->post('/orders', CreateOrderController::class);

$app->post('/login', [LoginController::class, 'onLogin']);
```

Class-and-method handlers are validated during route registration. Controllers
are resolved by the container, constructor dependencies are autowired, and
method parameters may receive request/response objects, container dependencies
and route parameters by name.

## Fifteen delivery tracks

| # | Track | Contract |
| --- | --- | --- |
| 1 | Application experience | Closures and structured applications share one runtime. |
| 2 | Router | Groups, prefixes, constraints, names, resources and compiled matching. |
| 3 | Dependency injection | Transient, singleton and request-scoped lifetimes. |
| 4 | Form Requests and DTOs | Authorization, validation and typed input hydration. |
| 5 | Resources | Stable `data`/`meta` domain response envelopes. |
| 6 | Route binding | Typed entity resolution with explicit lookup keys. |
| 7 | Middleware | Global, group and route layers with parameterized policies. |
| 8 | Authentication | Token strategies, policies, abilities and current principal. |
| 9 | Errors | Problem Details, integer error codes and safe production rendering. |
| 10 | OpenAPI | Contract generation, compatibility checks and typed clients. |
| 11 | Production primitives | Idempotency, transactions, cache, timeout, retry, circuit breaker, jobs, events, SSE and sockets. |
| 12 | Distributed rate limiting | Pluggable stores and safe client-key resolution. |
| 13 | Multi-tenancy | Request-scoped tenant resolution and isolation. |
| 14 | Testing | In-memory client, fakes, contract assertions and leak assertions. |
| 15 | Observability | Normalized route metrics, traces, logs and slow-request diagnostics. |

## Current implementation status

The `feat/api-2-foundation` development line implements:

- class-and-method controller handlers;
- boot-time typed configuration with deterministic secret redaction;
- constructor and method dependency injection;
- transient, singleton and request-scoped container lifetimes;
- named routes, built-in/custom constraints and per-route middleware;
- explicit and automatic HEAD/OPTIONS semantics with deterministic Allow headers;
- bounded route tables, paths and custom constraints with per-pattern PCRE
  match/depth budgets;
- composable prefixes and route groups;
- API resource route registration;
- Form Request authorization/validation and integer enum validation;
- Problem Details validation responses;
- JSON Resources and Resource Collections;
- PHPUnit and PHPStan level 9 verification;
- a committed 118-symbol public API baseline with executable SemVer checks;
- pluggable rate-limit stores with a bounded in-memory token bucket fallback;
- authenticators, request-scoped principals, abilities and authorization;
- strict signed bearer access tokens with temporal, issuer/audience checks,
  bounded signing-key rotation and pluggable revocation;
- idempotency and response-cache stores with bounded memory implementations;
- route model binding and custom binding resolvers;
- OpenAPI 3.1, compatibility checks and TypeScript/Kotlin/Swift clients;
- request-scoped tenancy and normalized request observations;
- deterministic request lifecycle observers with failure-aware reverse cleanup;
- a bounded opt-in development profiler that never retains headers or payloads;
- transactions, events, lease-based at-least-once jobs with retry/dead-letter
  transitions and allowlisted bounded JSON serialization, retry and
  circuit-breaker primitives;
- strict CORS, trusted-proxy IP resolution and cooperative deadlines;
- composable health checks and container-scope diagnostics;
- an in-memory test client with fluent response assertions;
- a canonical structured starter embedded by `pam init` and exercised through
  a clean dependency install and Embed SAPI smoke test;
- a reproducible router benchmark.

Eloquent is the official ORM and is integrated with Fiber-local connection
managers, events, transactions, deterministic request cleanup and per-request
query budgets with duplicate-query detection. Tenant model guards apply
fail-closed global scopes and prevent cross-tenant assignment. Redis,
broker, JWT and OpenTelemetry integrations remain contract-backed ecosystem
adapters until their production implementations complete the certification
suite.
