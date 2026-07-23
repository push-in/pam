# Security policy

## Supported versions

Until Pam reaches 1.0, security fixes are released only for the latest tagged
version. PHP itself must remain on an upstream-supported 8.4 patch release.

## Reporting a vulnerability

Do not open a public issue with an exploit or secret. Send the report privately
to the repository owner, including affected version, impact, reproduction, and a
suggested mitigation when available. Expect acknowledgement within 72 hours.

Secrets such as TLS private keys, Redis/NATS credentials, WebSocket resume secrets,
and application `.env` files must never be committed or included in diagnostics.
