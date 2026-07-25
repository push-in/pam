# PAM Laravel SaaS API reference

This is a complete, intentionally small Laravel 13 application demonstrating
PAM's production layout with thin controllers, Form Requests, services,
repositories, API Resources, integer-backed domain enums and endpoint tests.

```bash
cp .env.example .env
touch database/database.sqlite
composer install
pam artisan key:generate
pam artisan migrate
pam artisan pam:install --preset=api
pam dev pam.php
```

Create and list workspaces:

```bash
curl -X POST http://127.0.0.1:3000/api/workspaces \
  -H 'Content-Type: application/json' \
  -d '{"name":"Community"}'
curl http://127.0.0.1:3000/api/workspaces
```

Production uses `pam start pam.php --workers N`; queue and scheduler processes
come from the manifest published by `pam:install`.
