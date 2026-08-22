# Contributing to PAM

Thank you for helping improve PAM. Bug reports, compatibility findings,
documentation fixes, benchmarks and focused pull requests are welcome.

## Before you start

- Search existing issues and discussions.
- Use an issue for a behavior change or substantial design proposal.
- Never include credentials, `.env` files, private application code or
  vulnerability details in a public report.
- Read `SECURITY.md` before reporting a security issue.

## Local setup

Runtime contributors need Rust 1.88+, PHP 8.5 Embed development headers, a C
toolchain and Composer 2. Inspect manifests and lockfiles before changing or
installing dependencies. Run the relevant dry-run first.

```bash
cargo fmt --all -- --check
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo test --locked --all-targets --all-features -- --test-threads=1
```

For PAM Octane:

```bash
composer install --working-dir=packages/octane --dry-run
composer install --working-dir=packages/octane
composer --working-dir=packages/octane verify
cargo test --locked --test cluster --test server -- --test-threads=1
```

Run `scripts/package-release.sh validate` after changing a Composer package or
publication metadata. Performance changes must use the checked-in benchmark
protocol and disclose the source commit, dirty state, hardware, runtime versions,
worker count and complete results.

## Pull requests

Keep changes focused, add regression tests, update documentation and describe
operational or compatibility risks. All required CI checks must pass. Maintainers
may ask that broad work be split into smaller reviews.

By contributing, you agree that your contribution is licensed under Apache-2.0.
