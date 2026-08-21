# Changelog

All notable changes to the PAM application skeleton are documented here. The
skeleton follows Semantic Versioning independently from the PAM runtime.

## 2.0.0 - 2026-08-21

- target PAM API 2.0 and PHP 8.4;
- generate a named controller action, service, readonly snapshot, JSON Resource
  and sequential integer-backed readiness enum;
- validate typed configuration before startup;
- enable secure response headers by default;
- use PAM API's built-in in-memory test client;
- keep the published project and `pam init --template api` byte-for-byte aligned.
