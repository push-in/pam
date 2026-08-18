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
the `f5c6328` baseline binary measured 9,376,320 bytes. The verifier and its
direct Ed25519/SemVer dependencies produce a 9,585,216-byte binary: an increase
of 208,896 bytes, or 2.23%. Both builds used the same temporary Cargo target and
were removed after measurement.
