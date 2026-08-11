# Security policy

## Supported versions

PAM 1.x receives security fixes on the latest tagged minor/patch release. The
runtime supports the PHP, Laravel and Octane versions listed in the maintained
compatibility matrix; all underlying components must remain on upstream-supported
security releases. Pre-1.0 releases are no longer supported.

## Reporting a vulnerability

Do not open a public issue with an exploit or secret. Use the repository's
private GitHub Security Advisory reporting flow and include the affected version,
impact, reproduction, and a suggested mitigation when available. Expect
acknowledgement within 72 hours.

Secrets such as TLS private keys, Redis/NATS credentials, WebSocket resume secrets,
and application `.env` files must never be committed or included in diagnostics.
