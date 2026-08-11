# Contributing

Install dependencies and run the complete local contract:

```bash
composer install
composer verify
```

Before installing dependencies, inspect `composer.json` and `composer.lock`, run
`composer install --dry-run`, and confirm that your PHP version and extensions
match the lockfile. Runtime changes also require:

```bash
cargo test --locked --test cluster --test server -- --test-threads=1
```

Changes to request isolation, streaming or error handling must include a
regression test. Performance claims must use the same application, PHP version,
worker count, response bytes and hardware for every compared runtime.

Use GitHub Discussions for usage questions. File reproducible defects in the
main PAM issue tracker and report vulnerabilities through private GitHub
Security Advisories.
