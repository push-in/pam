# PAM HTTP architecture

PAM HTTP is the HTTP application kernel for the PAM runtime. It owns the request lifecycle,
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
| `pushinbr/pam-http` | HTTP kernel, router, middleware pipeline, lifecycle, errors, streaming |
| `pushinbr/pam-http-container` | Optional HTTP dependency injection implementation |
| `pushinbr/pam-http-validation` | HTTP validation, form requests, typed DTO hydration |
| `pushinbr/pam-http-auth` | HTTP authentication and authorization strategies |
| `pushinbr/pam-http-session` | Cookies, encrypted sessions, CSRF protection |
| `pushinbr/pam-http-openapi` | OpenAPI generation, compatibility checks, client generation |
| `pushinbr/pam-http-observability` | HTTP OpenTelemetry tracing, metrics, logging integration |
| `pushinbr/pam-http-testing` | In-process HTTP client, assertions, protocol conformance |
| `pushinbr/pam-http-eloquent` | HTTP-aware Eloquent lifecycle, tenancy, query budgets |
| `pushinbr/pam-jobs` | Job contracts, codecs, dispatchers, workers |
| `pushinbr/pam-http-psr` | PSR-7, PSR-15 and PSR-17 HTTP adapters |

WebSockets are owned by `pushinbr/pam-socket`; HTTP owns only the upgrade boundary. SSE and
HTTP streaming stay in PAM HTTP because they use the normal HTTP response lifecycle.

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
makes sense with PAM HTTP is named `pam-http-*`; an extension owned by PAM Native is named
`pam-native-*`. A short `pam-*` name is reserved for a capability that is genuinely independent
from every product kernel. PHP namespaces mirror ownership, such as `Pam\Http\Auth`.

`pam-api` is not a product name or package prefix. Public communication, commands, templates, and
documentation call this product **PAM HTTP**.
