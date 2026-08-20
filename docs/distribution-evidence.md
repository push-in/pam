# Signed clean-host distribution evidence

PAM distribution certification separates a source build from proof that an
immutable package installs, launches, upgrades, and rolls back on a clean target.
The portable schema is
[`schemas/distribution-evidence.schema.json`](schemas/distribution-evidence.schema.json).
Verify a bundle without network access:

```bash
pam distribution:verify evidence/distribution.json
pam distribution:verify evidence/distribution.json --json
```

The clean-host harness writes an unsigned draft without any of the three
`signing*`/`manifestSignature` fields. Sign only after every referenced file is
final:

```bash
umask 077
openssl rand -base64 32 > evidence.key
pam distribution:sign evidence/draft.json \
  --key evidence.key \
  --output evidence/distribution.json
pam distribution:verify evidence/distribution.json
```

The key file is a canonical padded-base64 32-byte Ed25519 seed. On Unix, PAM
rejects any group/other permission bit. It rejects symlink keys, bounded-key
violations, drafts that already contain signing fields, output aliases, and an
existing output file. The signer validates codes, timing, referenced sizes and
hashes before creating the result. It derives only the public key and identity
into the manifest; private key bytes are never serialized.

Schema version `1` uses sequential integer codes:

| Field | Codes |
| --- | --- |
| `surfaceCode` | `1` Runtime, `2` Native, `3` Desktop |
| `platformCode` | `1` Linux, `2` macOS, `3` Windows, `4` Android, `5` iOS |
| `architectureCode` | `1` x86_64, `2` arm64 |
| `packageCode` | `1` archive, `2` deb, `3` rpm, `4` AppImage, `5` dmg, `6` pkg, `7` msi, `8` nsis, `9` aab, `10` ipa |
| `checkCode` | `1` install, `2` launch, `3` first success, `4` upgrade, `5` rollback, `6` signature, `7` dependency inventory |
| `resultCode` | `1` passed |

Every certification contains all seven check codes exactly once. A failure is
retained in platform logs but is not a valid certification manifest.

Desktop certification has an additional executable boundary. Package code `1`
continues to represent PAM Desktop's supported portable host/application archive
and uses the shared clean-host install/launch/upgrade/rollback contract. Native
installers are distinct: Linux deb/rpm/AppImage, macOS dmg/pkg and Windows
msi/nsis must reference a bounded `platformVerification` document conforming to
[`schemas/desktop-platform-verification.schema.json`](schemas/desktop-platform-verification.schema.json).
The verifier rehashes that document and requires its installer digest to equal
the outer signed artifact digest. Sequential integer codes bind Apple Developer
ID plus notarization on macOS, Authenticode on Windows, or package/repository
signature verification on Linux. Signature, sandbox and interrupted-update
recovery results must all be `1` (passed); notarization is `1` on macOS and `2`
(not applicable) elsewhere. A generic `checkCode: 6` can no longer stand in for
this platform-specific Desktop proof.

Create the bounded report only after native tools have written their successful
raw evidence files beside the report. PAM infers signature/notarization codes
from the platform/package pair, hashes the installer and publisher certificate
itself, refuses inputs outside the report directory, rejects symlinks and
existing output, and embeds bounded descriptors for every proof:

```bash
pam distribution:desktop-report \
  --artifact evidence/files/application.dmg \
  --platform-code 2 --package-code 5 \
  --publisher-certificate evidence/files/developer-id.cer \
  --signature-proof evidence/files/codesign.log \
  --notarization-proof evidence/files/notarytool.json \
  --sandbox-proof evidence/files/sandbox-test.json \
  --update-recovery-proof evidence/files/update-recovery.json \
  --output evidence/files/platform-verification.json
```

`--notarization-proof` is mandatory only on macOS and rejected elsewhere. The
report generator does not accept caller-supplied “passed” codes. The native
workflow must invoke it only after every named platform command and recovery
test exits successfully, and must retain those exact outputs. The generator
binds those bytes but deliberately does not pretend it can reinterpret every
vendor-specific log format. `distribution:verify` rehashes the certificate and
every raw proof and checks that the certificate SHA-256 is the declared
publisher identity.

Portable evidence cannot carry `platformVerification`, so it cannot be mistaken
for OS-native trust. This contract deliberately does not claim that an installer exists merely
because the schema and verifier exist. A hosted certification must produce the
report from native platform tools, retain their raw output through the
provenance inventory, and pass clean-machine install, launch, upgrade and
rollback before the Desktop distribution claim moves to shipped.

The manifest references the exact candidate package, baseline package, and
dependency inventory with relative paths, sizes, and SHA-256 digests. It also
binds a provenance inventory covering the raw attestations and trusted root, and
records current and baseline commit hashes, the clean host image, installed
footprint, launch/first-success timing,
and a SHA-256 identity for the Ed25519 evidence key. PAM rejects unknown fields,
incompatible surface/platform/package combinations, traversal, backslashes,
absolute/non-portable paths, symlinks, empty files, size drift, digest drift, duplicate or
missing checks, failed checks, inconsistent timing, and invalid signatures.
The verifier also parses at most 256 canonical provenance-inventory entries and
rehashes every listed regular file, so a signed inventory cannot hide a modified
attestation bundle, policy result, or trusted root.

## Signing contract

`signingPublicKey` is standard padded base64 for the 32-byte Ed25519 public key.
`signingIdentitySha256` is the lowercase SHA-256 of those exact bytes.
`manifestSignature` is standard padded base64 for the 64-byte signature.

The signed payload is UTF-8 JSON canonicalized by parsing the complete manifest,
removing only `manifestSignature`, sorting object keys recursively through PAM's
JSON serializer, and emitting compact JSON. Arrays retain their declared order.
The verifier reconstructs those bytes and performs strict Ed25519 verification
before hashing referenced files. Signing raw pretty-printed input is not
compatible with version `1`.

The evidence key proves who certified the results; it does not replace the
platform package signature checked by `checkCode: 6`. Production workflows must
protect both authorities independently, publish the evidence-key fingerprint
through a second channel, and never place private keys in the artifact.

The release workflow expects the base64 seed in the protected secret
`PAM_DISTRIBUTION_EVIDENCE_KEY` and its independently published lowercase
fingerprint in the repository variable
`PAM_DISTRIBUTION_EVIDENCE_KEY_SHA256`. It refuses to publish when either is
missing or the derived identity differs. Configure the variable only after
verifying the fingerprint through the second channel; do not derive trust from
the public key carried inside the same manifest.

Planned rotation uses the optional repository variable
`PAM_DISTRIBUTION_NEXT_EVIDENCE_KEY_SHA256`. It must be empty or a distinct
lowercase SHA-256 identity. First publish that successor fingerprint through the
independent channel, configure the variable, and ship a bridge release still
signed by the current key. That binary pins exactly the current and successor
identities. Only after the bridge is available should operators replace the
protected signing seed and current fingerprint with the successor, then clear
or advance the next identity. A binary released before the bridge cannot trust
an unannounced replacement key; after unrecoverable key loss it must be upgraded
through a separately verified reinstall. The updater never downloads or persists
a new trust root.

Each Runtime release also publishes the exact signed `distribution.json` as
`pam-<tag>-<target>.update.json`. This compact copy contains no new authority:
its canonical Ed25519 signature is the certification signature. Official PAM
binaries pin the current identity and at most one pre-announced successor at
build time and accept the
manifest only when its identity, Runtime/archive codes, platform, architecture,
candidate path, bounded size and digest all match. The requested version remains
bound by the candidate binary's exact `pam <version>` identity check before
atomic activation. The installed SemVer is also a rollback floor for normal
updates: an older signed manifest is rejected before download. Operators must
name the older version and pass `--allow-downgrade` to authorize recovery.
`pam self-update --check` consumes this same authorization path and returns its
“available” status only after signature, pins, target and archive bounds pass.
New Runtime certification includes canonical `releaseVersion`; historical schema
1 evidence may omit the additive field and remains verifiable, but update
authorization requires it and requires an exact match with the requested tag.
The URL or release filename is never accepted as version authority.

New Runtime certification also signs `issuedAtUnix` and `expiresAtUnix` as a
paired, maximum 31-day freshness window. Automatically discovered updates,
including a response that claims the installed release is still latest, must
authorize the selected manifest inside that window; PAM permits at most five
minutes of future clock skew. This turns an indefinitely replayed GitHub
“latest” response into a fail-closed freshness error after the signed deadline.
An explicitly named version intentionally verifies the same immutable signature,
version, target and digest without the online freshness requirement, preserving
audited recovery and historical installation. Operators therefore need to ship
or re-certify an official release at least every 31 days for unattended update
discovery to remain available.

## Linux and macOS clean-host release gates

The release graph cannot publish before all four native-architecture entries in
the reusable Runtime certification matrix succeed: Linux x86_64/arm64 and
macOS x86_64/arm64. Each entry downloads its
candidate artifact from the same run and the greatest stable SemVer release
strictly older than the candidate, verifies both GitHub attestations, resolves the exact
Ubuntu container digest, then disables container networking. Inside that clean
image it extracts both packages, launches the candidate, obtains a real PHP
response, loads the complete PHP module list with bounded output and no startup
warnings, upgrades through an atomic symlink switch, and rolls back to the bound
baseline package. It records the normalized loaded-module list alongside a
sorted hash inventory of the bundled runtime surface, stores both raw
attestation bundles, their policy-verification JSON and
the contemporaneous trusted root for offline re-verification, signs and
independently verifies the manifest, retains the full bundle
for 30 days, and packages the same certification as an immutable release asset.
Tag-push and manual existing-tag releases share the same authority: certification
resolves the named tag to its commit, requires the checked-out revision to match,
and verifies the candidate attestation against `refs/tags/<tag>` rather than the
branch ref and SHA that initiated a manual dispatch. Release-list API ordering
is not trusted; bounded metadata is parsed locally and future, prerelease, and
malformed tags cannot become the rollback baseline.

The Linux matrix uses native x86_64 and arm64 GitHub runners, verifies `uname -m`
inside a network-disabled Ubuntu container, and records both the resolved
repository digest and architecture-specific image ID. The macOS matrix uses
ephemeral native Apple Silicon and Intel runners, checks `uname -m`, and binds
the evidence to the GitHub runner image/version plus the exact macOS version and
build. Both platforms exercise the relocatable `pam-run` package, atomic
upgrade, and rollback. Windows, Android, iOS, PAM Native applications, and PAM
Desktop installers remain open until equivalent protected-key workflows
exercise their real platform package and install mechanisms.

## PAM Desktop Linux host gate

[`desktop-linux-distribution.yml`](../.github/workflows/desktop-linux-distribution.yml)
is the reusable certification path for PAM Desktop's currently supported
portable Linux x86-64 host. Operators provide distinct canonical current and
baseline tags plus their immutable refs and the independently published
evidence-key identity. The job checks out both refs, proves each checkout is at
the named tag, downloads each exact archive plus its published checksum, and
runs the `test-host-archive.sh` implementation from the matching source version.
That upstream verifier checks the checksum, safe member types and paths, exact
file set, internal manifest bytes/digests, executable identity, rootless install
and exact-version uninstall before PAM verifies both GitHub attestations against
the PAM Desktop release workflow and source digest. Both published checksum
documents are retained inside the signed provenance inventory.

After resolving the Ubuntu 22.04 image digest, the job disables container
networking. It installs the baseline into isolated XDG roots, launches the real
host identity, installs the candidate, atomically points the managed command
back to the retained baseline, and records install/launch/upgrade/rollback
timings and installed bytes. It also records the candidate executable digest
and a normalized, bounded `ldd` inventory, failing certification when any
runtime library is unresolved. Raw attestation bundles, policy results and the
trusted root are retained and rehashed. The PAM evidence signer then emits a
surface `3`, Linux `1`, x86-64 `1`, portable archive `1` manifest and verifies it
again before the 30-day artifact can upload.

This gate certifies the published host archive, rootless installer and rollback
mechanism. A `--version` success is not a rendered application frame, and a
portable archive is not a deb/rpm/AppImage platform-signature claim. Native
installer and graphical clean-session certification remain separate gates.

## Bounded operation

- manifest: at most 256 KiB;
- package: at most 8 GiB, hashed in 64 KiB chunks;
- dependency inventory: at most 16 MiB;
- evidence paths: canonical and relative to the manifest directory;
- no network access and no dependency installation during verification.

A schema-valid or locally fabricated file is not hosted certification. Release
claims require a successful clean-host workflow using immutable package inputs,
the named host image, a protected signing identity, and retained raw platform
logs alongside this portable manifest.
