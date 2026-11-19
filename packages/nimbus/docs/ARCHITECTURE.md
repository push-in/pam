# Nimbus architecture

Nimbus is the HTTP application kernel for the PAM runtime. It owns the request lifecycle,
routing, middleware execution, handler resolution, responses, errors, and the contracts that
extensions use. It must remain useful without installing a database, queue, authentication
strategy, template engine, or a general-purpose framework.

## Permanent boundary

The package has three layers:

1. **Kernel** — `App`, router, route definitions, pipeline, request scope, handler resolution,
   lifecycle, and HTTP response primitives. These are the stable hot path.
2. **Extension contracts** — small interfaces and value objects used to attach capabilities to
   the kernel without coupling the kernel to their implementation.
3. **First-party packages** — database, authentication, validation, sessions, OpenAPI,
   observability, testing, and PSR interoperability. Applications install only what they use.

Code in the kernel may depend on PHP and `pushinbr/pam-contracts`. A first-party integration may
depend on the kernel, but the kernel must never depend on an integration. CI enforces this rule by
checking the production dependency set.

## Package direction

| Package | Responsibility |
| --- | --- |
| `pushinbr/nimbus` | HTTP kernel, router, middleware pipeline, lifecycle, errors, streaming |
| `pushinbr/nimbus-container` | Optional HTTP dependency injection implementation |
| `pushinbr/nimbus-validation` | HTTP validation, form requests, typed DTO hydration |
| `pushinbr/nimbus-auth` | HTTP authentication and authorization strategies |
| `pushinbr/nimbus-session` | Cookies, encrypted sessions, CSRF protection |
| `pushinbr/nimbus-openapi` | OpenAPI generation, compatibility checks, client generation |
| `pushinbr/nimbus-observability` | HTTP OpenTelemetry tracing, metrics, logging integration |
| `pushinbr/pam-http-testing` | In-process HTTP client, assertions, protocol conformance |
| `pushinbr/arcano` | ORM: entities, repositories, tenancy, migrations |
| `pushinbr/pam-jobs` | Job contracts, codecs, dispatchers, workers |
| `pushinbr/pam-http-psr` | PSR-7, PSR-15 and PSR-17 HTTP adapters |

WebSockets are owned by `pushinbr/pam-socket`; HTTP owns only the upgrade boundary. SSE and
HTTP streaming stay in Nimbus because they use the normal HTTP response lifecycle.

## Compatibility policy

The 2.x line keeps existing public symbols while integrations are extracted. Legacy integration
classes may require explicitly suggested packages and are deprecated only after their replacement
is published. The next major version removes those bridges from the kernel after an automated
migration path exists.

Every public-contract change must pass the API compatibility gate. Every hot-path change must pass
the router budget. A production install must pass a separate smoke test with development and
suggested packages absent.

## Naming rule

Repository names and Composer package names use the same product family. An extension that only
makes sense with Nimbus is named `nimbus-*` and uses the `Nimbus\` namespace; an extension owned
by PAM Native is named `pam-native-*`. `pam-http-*` packages are runtime-level HTTP
interoperability (PSR adapters, in-memory testing). A short `pam-*` name is reserved for a
capability that is genuinely independent from every product kernel.

`pam-api` and `pam-http` are not product names. Public communication, commands, templates, and
documentation call the framework **Nimbus**.
