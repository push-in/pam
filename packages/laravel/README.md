# PAM Laravel

Production tooling for Laravel 12 and 13 applications running on the
[PAM runtime](https://github.com/push-in/pam).

## Install

```bash
composer require pushinbr/pam-laravel
pam artisan pam:install --preset=api
pam artisan pam:check-production
```

Laravel package discovery registers the provider automatically. The installer
publishes `config/pam.php`, a multiprocess manifest and Docker Compose, systemd
and Kubernetes examples.

Available presets are `api`, `livewire`, `inertia` and `realtime`. The selected
preset is persisted as its stable integer enum value in `.pam/laravel.json`, so
automation has a versionable contract instead of inferring the stack.

## Production lifecycle

PAM Laravel adds a request state guard to the `web` and `api` middleware
groups. It rolls back leaked transactions, forgets authenticated users,
restores the process locale and clears resolved facades after every request.
Use strict mode in CI to turn detected state leakage into a failed request:

```dotenv
PAM_LARAVEL_STATE_GUARD=true
PAM_LARAVEL_STATE_GUARD_STRICT=true
```

Application-specific cleanup can implement `Pam\Laravel\Contracts\LifecycleHook`
and register it with `LifecycleManager`.

## Operations

```bash
pam artisan pam:health
pam artisan pam:leaks
pam artisan pam:capacity --memory-mb=1024 --worker-mb=96
pam artisan pam:process up
pam artisan pam:process status
pam artisan pam:process restart queue
pam artisan pam:deploy /srv/app/releases/20260725-120000
pam artisan pam:deploy --rollback
```

The default endpoints are:

- `GET /__pam/health` for readiness and liveness;
- `GET /__pam/metrics` for bounded process telemetry.

Set `PAM_LARAVEL_OBSERVABILITY_TOKEN` to require a bearer token on metrics.
The registry retains bounded route, query and state-violation data, preventing
the observability layer itself from becoming a memory leak.

## Zero-downtime releases

Prepare an immutable release inside `PAM_DEPLOY_ROOT`, run migrations and
framework cache generation in the release, then activate it with
`pam:deploy`. Activation uses an atomic symlink replacement and records the
previous target for rollback. Destructive schema changes should follow
expand/migrate/contract deployment discipline.

## Process manifest

`pam.processes.json` represents commands as arrays, not shell strings. This
keeps arguments unambiguous and avoids manifest command injection:

```json
{
  "processes": {
    "http": {
      "command": ["pam", "serve", "--host=127.0.0.1", "--port=3010"],
      "instances": 2
    }
  }
}
```

PAM's Laravel runtime remains conservative about request concurrency because
Laravel applications and third-party packages can hold mutable process-global
state. Scale with multiple isolated workers; certify lifecycle hooks under
load before opting into more aggressive concurrency.

## Support policy

The compatibility matrix tests PHP 8.4 with the maintained Laravel 12 release
and the current Laravel 13 release. The public compatibility registry lives in
the main PAM repository.
