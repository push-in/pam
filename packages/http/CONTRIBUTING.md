# Contributing to PAM API

Read the repository-wide [contribution guide](https://github.com/push-in/pam/blob/main/CONTRIBUTING.md)
and Code of Conduct before contributing.

For package changes:

1. add or update focused PHPUnit tests;
2. run `composer verify` in the package directory;
3. document public API and behavior changes;
4. include dependency preflight evidence for manifest or lock changes;
5. include reproducible before/after evidence for performance claims;
6. explain persistent-worker state, cleanup, cancellation and rollback risks.

Controllers in examples must remain thin. Validation belongs in Form Requests,
business rules in services, persistence in repositories and domain responses in
Resources. Coded status/type/state/kind/category values use sequential
integer-backed enums.
