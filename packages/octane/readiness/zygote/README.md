# Post-boot zygote harness

Reproduces the evidence in `docs/post-boot-zygote-evidence.md` against a
disposable Laravel app with MySQL 8.4 and Redis 7 containers.

1. Provision a Laravel 12 app with `provision_app` from `../lib.sh`, add
   `predis/predis`, point `.env` at MySQL/Redis (`SESSION_DRIVER=redis`,
   `CACHE_STORE=redis`, `REDIS_CLIENT=predis`), migrate and seed 100 users.
2. Copy `app/ZygoteProbeServiceProvider.php` to `app/Providers/`, register it
   in `bootstrap/providers.php`, copy `app/zygote.php` to `routes/` and
   `require` it from `routes/web.php`. With `ZYGOTE_PROBE_BOOT_IO=1` the
   provider opens MySQL, Redis, a log file and a raw socket while booting.
3. Run:

```bash
zygote_e2e.py --pam "$PAM" --app "$APP" --mode app --workers 16   # isolation + numbers
soak.py --pam "$PAM" --app "$APP" --minutes 30 --reload-every 60 --label soak   # soak
```

`soak.py` options add chaos (`--kill-every`, `--hog-every` with
`--scope-memory`, `--disconnect-every`), continuous reloads and leak sampling.
Use a short `SOAK_TMP` (Unix socket paths for connection balancing must fit
in 108 bytes).
