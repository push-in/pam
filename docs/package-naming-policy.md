# Package naming policy

PAM package names communicate ownership. Repository names and Composer package names use the same
suffix, so `push-in/pam-http-auth` publishes `pushinbr/pam-http-auth`.

## Families

- `pam` is only the runtime and process manager.
- `pam-contracts` contains genuinely cross-product runtime contracts.
- `pam-http` is the HTTP kernel; extensions use `pam-http-*`.
- `pam-native` is the native application kernel; extensions use `pam-native-*`.
- Future product kernels such as `pam-desktop` own their `pam-desktop-*` extensions.
- A short `pam-*` capability name is allowed only when it is independent from every product kernel.

PHP namespaces follow the same ownership boundary. New HTTP testing code uses
`Pam\Http\Testing`; new HTTP authentication code will use `Pam\Http\Auth`.

## Native modules are capabilities

Official native extensions expose one reusable platform capability. Applications compose them
directly, in the same way that React Native applications select independent native libraries:

- camera belongs to `pushinbr/pam-native-camera`;
- playback belongs to `pushinbr/pam-native-video`;
- media inspection and thumbnails belong to `pushinbr/pam-native-media`;
- telemetry belongs to `pushinbr/pam-native-observability`.

Do not create product-shaped packages such as `pam-native-feed`, `pam-native-social`,
`pam-native-commerce`, or `pam-native-streaming-app`. Do not hide unrelated capabilities behind
one dependency. A product template may demonstrate composition outside the official package
catalog, but it must not become the capability boundary or a required runtime dependency.

When a capability becomes large enough to have an independent native lifecycle, permission set,
vendor SDK, or release cadence, give it its own `pam-native-*` package. Existing package names are
kept as compatibility surfaces; new APIs follow the narrower ownership boundary.

## Renames

A published package is never silently repurposed. The replacement is published first. The old
name then receives a new metapackage release that requires the replacement and uses Composer's
`abandoned` field to provide a machine-readable migration destination. Namespace aliases may be
kept for one major migration window when existing application source would otherwise break.

New templates and documentation switch to the replacement immediately. CI installs both the new
name and its bridge until the migration window closes.

`pam-api` is not a current product name. The product is **PAM HTTP**.
