# Laravel Octane server benchmark

Reproducible comparison of PAM Octane, FrankenPHP, Swoole and RoadRunner
serving the **same** Laravel application through Laravel Octane.

```bash
packages/octane/readiness/run-local.sh bench        # or: bash bench/run.sh
```

What it does, in order (one server at a time, never in parallel):

1. Builds three images from `docker/` with the exact versions in
   `versions.env` (PAM release, FrankenPHP static binary, official
   `php:8.5.x-cli` + Swoole from PECL + RoadRunner binary). PHP is 8.5.12 for
   every server.
2. Provisions one Laravel app (`laravel/laravel` + `laravel/octane` +
   `pushinbr/pam-octane` + `spiral/roadrunner-*`), `APP_ENV=production`,
   `artisan optimize`, SQLite with 100 users. The probe routes are in
   `../matrix/overlay/routes/readiness.php`:
   `hello` (string), `json`, `db` (one indexed SQLite row), `view` (Blade, 20 items).
3. Starts each server with `--network host`, pinned to `SERVER_CPUS`, with
   `WORKERS` workers and recycling disabled.
4. Warms up, then runs `wrk -t WRK_THREADS -c WRK_CONNECTIONS -d DURATION
   --latency` pinned to `LOAD_CPUS` (disjoint from the server).
5. Records requests/s, p50, p99, socket errors and non-2xx responses, and the
   container's cgroup memory (current and peak).
6. Writes `machine.json` (CPU, kernel, governor, Docker, image IDs),
   `results.csv`, `results.md` and the raw `wrk` output.

Fairness notes:

- Same app, same PHP version, same OPcache/JIT settings (JIT tracing,
  128 MiB buffer, no timestamp validation), same worker count and CPU set.
- Swoole runs with one task worker (Octane requires it); its HTTP workers are
  `WORKERS`.
- Numbers are only comparable within one run on one machine. Publish
  `machine.json` with any result.
