# Pam application

> This branch targets PAM API 2.0. Use the published `1.x` skeleton until the
> PAM API 2.0 prerelease is available on Packagist.

```bash
pam composer install
pam composer dev
```

The API starts at `http://127.0.0.1:3000`.

The starter intentionally demonstrates the structured PAM API path: a named
controller method orchestrates a service and returns a JSON Resource. Closures
remain available for small endpoints, but application rules should live in
services and persistence should live in repositories.

Run the in-memory application test inside Pam's Embed SAPI:

```bash
pam composer test
```

The generated application and this published skeleton share the same source
files:

```text
index.php
src/
├── Http/Controllers/PingController.php
├── Http/Resources/PingResource.php
└── Services/
    ├── ReadinessService.php
    ├── ReadinessSnapshot.php
    └── ReadinessStatus.php
tests/ApplicationTest.php
```

`index.php` validates typed configuration before listening and installs secure
response headers. The controller only orchestrates the request, the service
creates the application result, and the Resource owns its HTTP representation.

## License

The PAM skeleton is open source under the
[Apache License 2.0](LICENSE). Application files copied from this
skeleton and code emitted by PAM generators may be used, modified, sublicensed,
and distributed under terms of your choice, as stated in the Additional Use
Grant.
