# Pam application

```bash
pam composer install
pam dev index.php
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

## License

The PAM skeleton is open source under the
[Apache License 2.0](LICENSE). Application files copied from this
skeleton and code emitted by PAM generators may be used, modified, sublicensed,
and distributed under terms of your choice, as stated in the Additional Use
Grant.
