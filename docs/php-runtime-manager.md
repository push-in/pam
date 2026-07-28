# PHP Runtime Manager

PAM is the single owner of PHP across every runtime built on the platform.
Native, desktop and server adapters inherit PHP from PAM; they do not download,
compile or publish their own PHP distributions.

Mobile applications declare a supported PHP series in `pam-native.json`:

```json
{
    "runtime": {
        "php": "8.5",
        "channel": "stable"
    }
}
```

PAM resolves that request through `runtime/catalog.json`, which pins the exact
official PHP source URL, SHA-256, Android API, NDK, extension surface and PAM
runtime revision. The resolution is written to
`.pam-native/runtime.lock.json`.

```bash
pam mobile runtime:list .
pam mobile runtime:use 8.5 .
pam mobile runtime:info .
pam mobile runtime:update .
```

Runtime IDs use `<php-version>-r<revision>`. A PHP security update changes the
PHP version and resets the revision. A PAM portability patch, build flag or
packaging correction increments only the revision.

Release builds consume the exact lock. PAM release automation recompiles every
Android ABI from the verified source and publishes the runtimes inside the PAM
distribution with build provenance.
