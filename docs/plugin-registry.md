# Signed plugin registry

PAM registry schema 1 authenticates plugin metadata before any package manager,
native build tool or Desktop host sees an artifact. Verification is offline and
does not treat HTTPS, a Composer repository or a marketplace account as a trust
root.

The official production root has not undergone its key ceremony yet. Until its
fingerprint is published through PAM release artifacts and an independent
channel, callers must provide the expected 32-byte SHA-256 explicitly:

```bash
pam registry verify \
  --root pam-plugin-root.json \
  --root-sha256 "$PAM_PLUGIN_ROOT_SHA256" \
  --catalog pam-plugin-catalog.json \
  --minimum-sequence 42 \
  --json
```

The reproducible custody, detached-signing and publication procedure is defined
in the [registry operations runbook](registry-operations.md). `pam registry
payload` emits the verifier's exact canonical bytes, while `pam registry key-id`
derives a key identity from a validated raw Ed25519 public key. Neither command
loads or generates private keys.

`--minimum-sequence` is the last sequence accepted by the caller. Persist it in
CI or the package client and never decrease it; this blocks a validly signed
catalog rollback within the seven-day metadata lifetime.

## Trust root and rotation

A root contains `schemaVersion: 1`, an HTTPS registry identity, positive
`generation`, `issuedAtUnix`, `expiresAtUnix`, a signature `threshold`, sorted
keys and sorted signatures. Root validity is limited to 366 days. Key state
codes are sequential integers:

| Code | State | May sign current metadata |
| ---: | --- | --- |
| 1 | Active | Yes |
| 2 | Retired | No |
| 3 | Revoked | No |

`keyId` is the lowercase SHA-256 of the raw 32-byte Ed25519 public key. Initial
trust requires both the caller-pinned hash of the complete root file and the
root's own threshold signatures.

Rotation is one generation at a time:

```bash
pam registry rotate \
  --root current-root.json \
  --root-sha256 "$CURRENT_ROOT_SHA256" \
  --next-root next-root.json
```

The exact next-root payload must meet the current root's threshold through
`previousSignatures` and the next root's threshold through `signatures`. This
prevents a single new key from appointing itself and prevents the old quorum
from installing a root the new quorum did not approve. Emergency compromise is
handled by rotating with remaining active keys and publishing the affected
plugin releases in the revocation list.

Projects adopt a verified rotation together with its first catalog:

```bash
pam registry adopt \
  --project . \
  --next-root registry/root-v2.json \
  --next-catalog registry/catalog-v2.json \
  --json
```

Both document paths must be normalized and project-relative. Adoption verifies
the pinned current root, the old and new root quorums, the exact one-generation
advance, the new catalog quorum and the project's accepted sequence floor before
writing anything. It preserves Native/Desktop protocol selections while replacing
the root path, computed root fingerprint, catalog path, generation and sequence.

The update is interruption-recoverable through
`.pam/plugin-registry-rotation.json`. The receipt records operation code `1`, the
previous state and the fully verified next configuration/state before the first
replacement. On the next registry read, PAM either restores the previous state if
the configuration remained old or completes the new state if the configuration
was committed. A mismatched or malformed receipt fails closed.

## Catalog and compatibility

Catalogs expire after at most seven days and carry positive monotonic
`sequence`, the exact root generation, sorted releases, sorted revocations and
threshold signatures. Each release declares:

- Composer `vendor/package` and strict SemVer;
- artifact kind code: `1` Composer package, `2` Native archive, `3` Desktop
  executable;
- HTTPS artifact URL and lowercase SHA-256;
- publication timestamp;
- sorted surface codes: `1` Server, `2` Native, `3` Desktop;
- a PAM SemVer requirement and exact Native/Desktop protocol when applicable.

Resolution verifies the complete chain before selecting the highest compatible
non-revoked SemVer release:

```bash
pam registry resolve \
  --root pam-plugin-root.json \
  --root-sha256 "$PAM_PLUGIN_ROOT_SHA256" \
  --catalog pam-plugin-catalog.json \
  --minimum-sequence 42 \
  --package pushinbr/pam-native-camera \
  --surface-code 2 \
  --pam-version 1.0.3 \
  --native-protocol 1 \
  --json
```

Revocation reason codes are `1` security vulnerability, `2` signing-key or
publisher compromise, `3` policy violation and `4` withdrawn release. A catalog
is invalid if it advertises the same package/version as both installable and
revoked.

## Authenticated PAM Desktop host

A Desktop project with `pam-registry.json` resolves
`pushinbr/pam-desktop-host` on surface `3`, artifact kind `3` and Desktop
protocol `6` before any Desktop command starts. The catalog URL points to the
standalone `pam-desktop-<version>-<target>` executable published and attested by
the Desktop release workflow, not to the portable archive.

PAM downloads only over HTTPS/TLS 1.2 or newer, caps the executable at 512 MiB,
checks the catalog SHA-256 and then executes `--version` to require the exact
`pam-desktop <version>` identity. It publishes the verified bytes atomically to
`.pam/desktop-host/<sha256>/pam-desktop`, records the registry, root generation,
catalog sequence, package, version and digest in
`.pam/desktop-host.artifact.json`, and removes superseded host directories.
The project sequence advances only after a valid executable and provenance are
durable. Without `pam-registry.json`, the existing sibling, `PATH` and
`PAM_DESKTOP_BINARY` development lookup remains unchanged.

## Enforcing signed releases in `pam add`

Place `pam-registry.json` at the project root to opt into authenticated installs:

```json
{
  "schemaVersion": 1,
  "rootPath": "registry/root.json",
  "rootSha256": "<64 lowercase hexadecimal characters>",
  "catalogPath": "registry/catalog.json",
  "nativeProtocol": 1,
  "desktopProtocol": 1
}
```

Paths must be normalized, relative to the project, and resolve to regular files.
Only the protocol matching the project surface is required. API, Laravel, and raw
runtime projects require neither protocol field.

With this file present, `pam add` refuses unsigned resolution. It verifies the
root and catalog quorum, expiration, revocations, PAM compatibility, exact surface
protocol, and the previously accepted catalog sequence. Composer receives the
resolved version rather than a floating range. Before Composer mutates the project,
PAM downloads the bounded HTTPS artifact to a temporary file and verifies its
SHA-256. The verified ZIP is promoted into the project-local bounded artifact
store and exposed through an ephemeral canonical Composer `artifact` repository,
so both dry-run and installation consume those exact bytes instead of downloading
the URL again. After installation, `composer.lock` must contain that exact version
and local artifact path; otherwise the command fails. This follows Composer's
official [artifact repository contract](https://getcomposer.org/doc/05-repositories.md#artifact).

The store keeps at most one archive per package: once an updated release has been
installed and locked successfully, PAM removes superseded archives for that same
package. The ephemeral Composer home and its cache are always removed after the
operation, including failed installs, while archives for other installed packages
remain available for reproducible `composer install` runs.

The accepted registry identity, root fingerprint/generation, and catalog sequence
are stored atomically in `.pam/plugin-registry-state.json`. A later catalog cannot
move below that sequence. `pam add` refuses a changed root fingerprint; operators
can inspect a candidate with `pam registry rotate` and commit it transactionally
with `pam registry adopt`.

## PAM Native Android runtime

An authenticated Native project resolves the Android runtime as
`pushinbr/pam-android-runtime`, surface code `2`, artifact kind code `2`, and the
CLI's exact Native protocol (`1` for the current release). A different configured
protocol or artifact kind is rejected before network access.

`pam doctor --fix` and `pam mobile runtime:install` then download the catalog's
HTTPS `artifactUrl` once, cap it at 1 GiB, and compare its bytes with the signed
SHA-256. The sibling release checksum is deliberately not requested in this mode.
Only after the existing archive path checks, runtime/ABI validation, extraction,
and installation succeed does PAM advance `.pam/plugin-registry-state.json` to
the catalog sequence. Provenance is recorded in
`.pam-native/android-runtime.artifact.json`; a different registry root, catalog
sequence, version, package or artifact hash forces reinstallation even when the
shared runtime files already exist. The temporary archive and extracted tree are
removed on success or failure.

Projects without `pam-registry.json` retain the compatible release-asset flow and
its bounded sibling checksum. This fallback can be removed after an official root
ceremony and catalog become universally available.

## Canonical signed bytes

Signatures cover compact UTF-8 JSON without trailing newline. Object fields use
the exact order below; arrays must already be sorted and unique. Integers are
base-10 JSON integers, strings use standard JSON escaping, optional protocol
fields serialize as `null`, and signature arrays are excluded.

Root payload order:

```text
schemaVersion, registry, generation, issuedAtUnix, expiresAtUnix, threshold, keys
```

Catalog payload order:

```text
schemaVersion, registry, rootGeneration, sequence, generatedAtUnix,
expiresAtUnix, plugins, revocations
```

Documents are capped at 1 MiB and must be regular non-symlink files. Unknown
fields, uppercase hexadecimal values, unsorted entries, duplicate signatures,
credentials in URLs, fragments, invalid timestamps and insufficient quorum are
rejected before resolution.

Private signing keys are intentionally outside PAM. Production keys should be
generated and held by an offline or hardware-backed ceremony with independent
operators; committing fixture seeds or accepting a locally generated root as
official would defeat the trust model.

## Footprint evidence

On Linux with the repository release profile (`thin` LTO, symbols stripped),
the `cf3b28d` baseline binary measured 9,376,320 bytes. The verifier and its
direct Ed25519/SemVer dependencies produce a 9,585,216-byte binary: an increase
of 208,896 bytes, or 2.23%. Both builds used the same temporary Cargo target and
were removed after measurement.
