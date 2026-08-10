# PAM PHP runtime for iOS

Run this builder on macOS with Xcode selected:

```bash
runtime-builder/ios/build.sh --php 8.5 all
```

It verifies the PHP source against `runtime/catalog.json`, cross-compiles PHP
Embed and the PAM Rust engine for an arm64 device plus arm64/x86_64 simulators,
and creates reproducible `PamPhp.xcframework` and
`PamNativeEngine.xcframework` bundles under `runtime/ios/<runtime-id>/`.

The runtime has no CLI, network stack or dynamic extensions. Platform services
remain implemented by PAM Native modules. Xcode, Rust and the Apple Rust targets
are required; `pam doctor --fix` installs only the missing Rust targets.
