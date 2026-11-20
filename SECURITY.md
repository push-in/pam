# Security policy

## Supported versions

| Component | Supported | Fixes ship in |
| --- | --- | --- |
| PAM runtime (`pam` binary, `native/` extension, `runtime/`) | latest 2.2.x release | the next 2.2.x patch release |
| Nimbus (`pushinbr/nimbus`) | latest 0.x minor | the next patch of that minor |
| Arcano ORM (`pushinbr/arcano`) | latest 0.x minor | the next patch of that minor |
| PAM 2.1 and older, Nimbus and Arcano older minors | no | upgrade |

The runtime supports the PHP, Laravel and Octane versions listed in the maintained
compatibility matrix; the bundled PHP runtimes track upstream security releases,
and a PHP security release is followed by a PAM release with the new runtimes.

## Reporting a vulnerability

Do not open a public issue with an exploit or secret. Use the repository's
private GitHub Security Advisory reporting flow ("Report a vulnerability" on the
Security tab) and include the affected component and version, impact,
reproduction, and a suggested mitigation when available.

- Acknowledgement within 72 hours.
- An assessment (accepted or not, severity) within 7 days.
- A fix or mitigation for critical and high severity issues within 30 days, then a
  GitHub Security Advisory with the fixed versions and credit to the reporter
  unless they prefer otherwise. Please give us that time before disclosing.

## Hardening defaults

These defaults are part of the security contract; changing them is a
behaviour change listed in the changelog:

- `X-Forwarded-For` is honoured only from `trustedProxies` / `PAM_TRUSTED_PROXIES`
  (addresses or CIDR ranges), and the client is the rightmost untrusted entry.
- The worker `metricsPath` answers loopback peers only, unless `metricsToken`
  or `metricsPublic` is set.
- WebSocket upgrades from another site's page are refused unless the origin is
  in `websocketOrigins` (or `corsOrigins`).
- Request bodies held by PHP are bounded per worker (`maxInflightBodyBytes`).
- Errors never expose messages or traces unless `exposeErrors` is enabled.

Secrets such as TLS private keys, Redis/NATS credentials, WebSocket resume secrets,
metrics tokens, `APP_KEY` and application `.env` files must never be committed or
included in diagnostics.
