# Changelog

All notable PAM API changes are documented here. The package follows Semantic
Versioning and the compatibility policy in [UPGRADE.md](UPGRADE.md).

## 2.0.0 - 2026-08-21

### Added

- class-and-method controller handlers and structured route groups;
- transient, singleton and Fiber-local request-scoped container lifetimes;
- Form Requests, DTO hydration, Resources and integer enum validation;
- Eloquent as the default ORM with Fiber-local connections, migrations,
  transactions, events, query budgets, health checks and tenant guards;
- signed bearer access tokens with bounded key rotation and revocation;
- typed configuration with deterministic secret redaction;
- automatic `HEAD`/`OPTIONS` semantics and deterministic `Allow` headers;
- final `HEAD` body suppression with status and header preservation;
- bounded route configuration and PCRE match/depth budgets;
- an executable reflection-derived public API compatibility baseline enforced
  by Composer and required CI;
- an executable upgrade fixture bound to the published 1.0.2 source commit;
- built-in fluent test assertions for successful responses, headers and exact
  JSON payloads, used by the canonical structured starter;
- request lifecycle observers and a bounded privacy-safe development profiler;
- a lease-based job queue contract and bounded worker with retries, expired
  lease recovery and dead-letter transitions;
- allowlisted, versioned and size-bounded JSON job serialization;
- OpenAPI 3.1, compatibility checks and generated clients;
- production primitives for auth, rate limiting, idempotency, caching,
  resilience, tenancy, events, jobs, health and observability.

### Changed

- PHP 8.4 is the minimum supported runtime;
- Eloquent/Illuminate 13 is part of the default PAM API installation;
- persistent-worker request isolation is a public correctness contract.

Release evidence includes the executable public API baseline, PHPStan level 9,
the locked PHPUnit suite, isolated package split verification and the generated
starter installation smoke in required CI.

## 1.0.2 - 2026-08-20

- Published the initial standalone PAM API package.
