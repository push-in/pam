# PAM PHP runtime for iOS

Run this builder on macOS with Xcode selected:

```bash
runtime-builder/ios/build.sh --php 8.5 all
```

It verifies the PHP source against `runtime/catalog.json`, cross-compiles PHP
Embed and the PAM Rust engine for an arm64 device plus arm64/x86_64 simulators,
and creates reproducible `PamPhp.xcframework` and
`PamNativeEngine.xcframework` bundles under `runtime/ios/<runtime-id>/`.

OPcache is linked statically (PHP 8.5 core) without JIT, which iOS code
signing forbids. A cross build cannot run OPcache's shared memory probes, so the
builder declares the `mmap(MAP_ANON)` model iOS supports and fails if a slice
lacks it, lacks the file cache or carries JIT. The host enables OPcache with a
persistent file cache in `Library/Caches`.

The runtime has no CLI, network stack or dynamic extensions. Platform services
remain implemented by PAM Native modules. Xcode, Rust and the Apple Rust targets
are required; `pam doctor --fix` installs only the missing Rust targets.
