# PAM Supremacy delivery contract

PAM Supremacy is the evidence-backed program for making PAM a mature,
API-first, Eloquent-first and persistent-worker-first PHP framework. This is a
delivery contract, not a claim that every capability is currently available.

The creation of a family of official packages is deliberately out of scope
until its dependency, ownership, release and compatibility architecture is
approved separately. Stable extension contracts and the plugin architecture
remain in scope.

## Competitive baseline

PAM measures itself against the strengths users already receive elsewhere:

- Laravel: Eloquent, batteries-included application DX, queues and operational tooling;
- Symfony: stable components, security, diagnostics and the Web Profiler;
- Fastify: compiled schema validation/serialization and encapsulated plugins;
- Hyperf and OpenSwoole: coroutine-aware services, pools and concurrency;
- Spiral and RoadRunner: supervised workers, jobs, transports and metrics;
- NestJS: modules, guards, interceptors and consistent application structure.

Comparisons must use maintained versions, official documentation, identical
workloads and published raw evidence. A synthetic hello-world result cannot be
used to claim application-level superiority.

## Non-negotiable architecture

1. Request, principal, tenant, transaction and mutable ORM state are scoped to
   the current request Fiber.
2. Controllers orchestrate, services own use cases, repositories own
   persistence, Form Requests validate input and Resources serialize output.
3. Status/type/state/kind/category values are sequential integer-backed enums.
4. Public contracts are typed, versioned and checked for compatibility.
5. Unbounded work requires a limit, cancellation, backpressure and telemetry.
6. Debug tooling is disabled and unreachable in production by default.
7. Performance, compatibility and security are executable release gates.

## Delivery streams

| Stream | Required outcome |
| --- | --- |
| Runtime | Structured concurrency, cancellation, backpressure, graceful lifecycle, protocol coverage, warmup and leak-free workers. |
| Data | Fiber-safe Eloquent, migrations, savepoints, pools, read/write routing, tenancy, query budgets, outbox and health telemetry. |
| HTTP | Compiled routes, complete semantics, negotiation, uploads, streaming, SSE, WebSockets and PSR interoperability. |
| Contracts | JSON Schema 2020-12, OpenAPI 3.1, compiled validators/serializers, compatibility checks, mocks and typed clients. |
| Security | Tokens, API keys, OAuth/OIDC/passkey foundations, policies, distributed throttling, redaction, audit and supply-chain evidence. |
| Async | Durable queues, retries, dead letters, batches, scheduling, locks, idempotency, outbox/inbox and observable workflows. |
| Modules | Encapsulation, deterministic dependency graph, capabilities, lifecycle, compatibility metadata and certification harness. |
| DX | Interactive CLI, makers, dry-runs, typed config, diagnostics, codemods, upgrades, REPL and actionable errors. |
| Lens | Development profiler for requests, queries, cache, jobs, events, I/O, Fibers, CPU and memory plus production OpenTelemetry. |
| Quality | Unit, integration, contract, property, fuzz, mutation, race, leak, chaos and soak suites across maintained matrices. |
| Performance | Reproducible CRUD, database, Redis, streaming, queue, WebSocket, cold-start, P99 and memory comparisons. |
| Community | Tutorials, cookbook, reference, examples, governance, RFCs, LTS/security policies, signed releases and adoption trials. |

## PAM 2.0 release gates

PAM 2.0 cannot be called stable until all of the following are evidenced:

- clean installation reaches a working API in under five minutes;
- public API compatibility tooling reports no unexplained break;
- PostgreSQL, MySQL and SQLite integration suites pass;
- a complete application demonstrates auth, Eloquent, validation, resources,
  distributed state, queues, telemetry and deployment;
- a 24-hour sustained test shows no unbounded RSS or retained request state;
- benchmark artifacts publish P50/P95/P99, throughput, errors and memory on
  pinned hardware and workloads;
- gates reject more than 5% unexplained performance loss;
- core coverage is at least 90% and mutation score is at least 80%;
- fuzz, race, cancellation, timeout, recovery and upgrade tests pass;
- releases have checksums, signatures, provenance, SBOM and rollback evidence;
- docs contain a tutorial, production guide, cookbook, API reference,
  troubleshooting and upgrade guides;
- LTS, SemVer, deprecation, security response and maintainer policies exist;
- at least two external adoption trials complete without author intervention.

## Evidence states

Every tracked capability uses one of these sequential states:

1. `Defined` — contract and acceptance criteria exist.
2. `Implemented` — production code and focused tests exist.
3. `Verified` — full local and hosted gates pass.
4. `Published` — release artifacts and public documentation are available.
5. `Adopted` — an external application has validated the workflow.

Documentation must never describe a capability at a higher state than its
machine-readable release evidence.

## Execution order

1. Freeze contracts and release gates.
2. Close runtime, request-isolation and data correctness.
3. Compile routing, container and contract processing.
4. Complete security and distributed state.
5. Complete async work and workflows.
6. Deliver CLI, skeletons, profiler and diagnostics.
7. Complete protocols, modules and extension contracts.
8. Prove compatibility, security and sustained performance.
9. Publish documentation, governance and signed distribution.
10. Run external adoption trials and only then declare PAM 2.0 stable.

