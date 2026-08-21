# Start with PAM

PAM is the single command-line interface for the whole ecosystem: HTTP APIs,
Laravel, realtime services, PAM Native mobile applications, PAM Desktop, data
packages, plugins, development tools, and production builds.

You install PAM once and use `pam` for the rest of the project lifecycle.

## Install

```bash
curl --proto '=https' --proto-redir '=https' --tlsv1.2 \
  --connect-timeout 15 --max-time 60 --max-filesize 1048576 \
  --fail --silent --show-error --location \
  https://github.com/push-in/pam/releases/latest/download/install.sh | sh
```

```bash
pam --version
pam doctor
```

Official releases include a private PHP runtime and acquire a verified Composer
PHAR when required. A separately installed PHP CLI, Composer, Rust, Gradle, or
Xcode is not needed merely to explore PAM. Platform build tools are diagnosed
when a project actually targets that platform.

The installer bounds release metadata to 1 MiB, checksums to 16 KiB and runtime
archives to 1 GiB, with explicit connect/total deadlines and HTTPS-only official
redirects. A checksum document must contain exactly one lowercase SHA-256 entry
for the expected archive; PAM computes the digest itself instead of allowing the
downloaded document to select local paths. Extraction does not inherit archive
owners or permissions, rejects links and special files, and refuses a package
that expands beyond 4 GiB or 100,000 filesystem entries. Failed extraction is
removed by the installer trap. Extraction also runs with a 15-minute CPU limit
and a portable per-file ceiling no greater than 4 GiB, so one entry cannot grow
past the total package budget before the post-extraction measurement.
`pam self-update --check` applies the same bounded metadata policy, requires an
HTTPS release API even when overridden, and reports a newer release only after
verifying its compact manifest with the pinned identity and exact target. It is
an authorization preflight, not an unauthenticated version comparison.
Automatic discovery also requires the selected manifest's signed issue/expiry
window to be current, even when GitHub claims the installed version is latest;
the maximum window is 31 days with five minutes of future clock skew. Naming a
version explicitly keeps historical recovery available while still enforcing
its signature, exact version, target, size and digest.
Official release binaries additionally contain the independently published
distribution-key identity. Before self-update starts the embedded installer,
the running binary downloads the target's bounded `.update.json`, verifies its
canonical Ed25519 signature and exact Runtime/platform/architecture codes, and
passes the authorized archive digest to the installer. The installer requires
that digest to match both the strict checksum document and the downloaded bytes.
Source builds without a compile-time `PAM_UPDATE_SIGNING_IDENTITY_SHA256` fail
self-update closed instead of learning trust from the downloaded manifest.
Official bridge releases may also pin one distinct
`PAM_UPDATE_NEXT_SIGNING_IDENTITY_SHA256`; this bounded successor enables key
rotation without accepting an authority learned from the network.
The embedded installer copy is created atomically at mode `0700`; a scoped guard
removes it if writing, syncing or permission hardening fails. Launch failures,
non-zero installer exits and successful updates all require cleanup to succeed
instead of silently accumulating partial scripts in the system temporary area.
The running binary also rejects any signed release older than its own SemVer, so
replaying a legitimate historical manifest cannot turn a normal update into a
downgrade. Recovery to an older version requires both an explicit version and
`--allow-downgrade`; the flag is invalid with `--check` or automatic latest
selection.
Before activation, the candidate must report the exact requested PAM version;
the probe has a five-second wall/CPU budget, a bounded regular-file transcript
and exactly one expected identity line. Silence, output flooding, timeout or
extra diagnostics fail before activation. The launcher symlink is then replaced
with a same-directory atomic rename.
The version directory uses a destination-filesystem `.installing` lock/stage as
well, so cross-filesystem copies and concurrent installers are never published
partially. After successful activation, PAM retains the current release and the
two newest recognized previous releases for rollback; older releases for that
platform are removed, while symlinks and unrelated directories are ignored.
Release selection accepts canonical `vMAJOR.MINOR.PATCH[-PRERELEASE]` only; the
CLI and installer reject ambiguous leading zeros and build metadata.

For Android projects, `pam doctor --fix` installs PAM's SHA-256-verified PHP
runtimes and native engines. Projects configured with the signed PAM registry
also authenticate the artifact URL, compatibility, Native protocol and catalog
sequence before installation. Application developers do not need a Rust
toolchain unless they intend to rebuild the engine itself.

## Create a project interactively

```bash
pam
```

Or choose a preset explicitly:

```bash
pam init my-api --template api
pam init my-laravel-app --template laravel
pam init my-native-app --template mobile
pam init my-desktop-app --template desktop
pam init my-product --template product
```

The `product` template creates one bounded workspace with independently runnable
Server, Native, and Desktop applications plus `packages/contracts`, a local PHP
package shared by all three. Its first executable flow exposes the same typed
readiness snapshot through HTTP, native controls, and the authenticated Desktop
bridge. Surface and readiness variants remain sequential integer-backed enums;
the generated transports never introduce string discriminators.

The same package owns `design-tokens.json` and a fail-closed JSON Schema.
Light/dark themes use sequential integer mode codes, semantic color roles, a
4/8 spacing rhythm, bounded motion, and a 48-unit minimum touch target. Root
`pam.json` publishes `workspace.designTokenPath`, so tooling locates the
contract without guessing. The generated Native app reads that bounded document,
validates its exact schema/mode order and derives both PAM Mobile UI themes from
the framework defaults, so unspecified framework roles remain forward-compatible.
The generated Desktop worker exposes the same bounded contract through an
authenticated typed command; its renderer validates the exact response, applies
semantic CSS variables, follows system light/dark changes, and retains its safe
built-in theme if loading fails. Full screenshot parity certification remains a
separate gate.

Use `pam clean --dry-run` at the workspace root to inspect cross-surface caches
and builds, then `pam clean --all` to remove them without touching source,
Composer manifests, lockfiles, or the shared contract.

The Desktop preset opens with a responsive PAM workbench that demonstrates the
typed PHP command bridge, explicit native capabilities, signed updates and the
runtime inspector. Its local-only assets include keyboard focus, reduced-motion,
high-contrast and forced-color behavior; the runtime status becomes “online”
only after the authenticated bridge handshake.

Every generated project contains `pam.json`. This lets the CLI select the right
implementation when you use the same short commands everywhere:

```bash
cd my-native-app
pam doctor
pam dev
pam test
pam build
```

For a Native project, install `.pam` syntax highlighting, diagnostics,
completion, hover help, and format-on-save in VS Code with:

```bash
pam editor:install vscode
```

Neovim and Helix users receive safe managed configuration without replacing
their existing editor setup:

```bash
pam editor:install neovim
pam editor:install helix
```

## Ship and update

The same distribution command creates the platform-appropriate artifact. API,
Laravel, and raw runtime projects receive a versioned `tar.gz`; PAM Native
produces signed Android AAB/APK artifacts or an exported iOS IPA. Every binary
receives a neighboring SHA-256 file.

```bash
pam release --check
pam package
pam self-update --check
```

`pam release` runs doctor, lint, and tests before packaging. Native releases
also require the platform signing environment before Gradle creates the AAB or
Xcode exports the IPA.

Product workspaces package each surface in its owning directory, then create a
deterministic cross-surface release index from the workspace root:

```bash
cd apps/server && pam package
cd ../native && pam package
cd ../desktop && pam package
cd ../.. && pam package
pam release:verify
```

Before trusting a Desktop build or installer operation, inspect the host
boundary without downloading or launching the application:

```bash
pam desktop host:doctor . --json
```

The schema-1 report uses `surfaceCode: 3`; result code `1` means the signed
registry release, persisted provenance, exact SHA-256 and
`pam-desktop <version>` identity all agree. Result code `2` needs attention.
Source codes are `1` signed registry, `2` explicit environment binary, `3`
sibling binary and `4` PATH. Fallback binaries may be useful during local
development but are never reported as authenticated installer evidence. The
identity probe is bounded to five seconds and 4 KiB on each output stream; a
stalled or noisy host process group is terminated and fails closed instead of
hanging Doctor or first acquisition indefinitely. In signed-registry mode,
Doctor verifies the catalog SHA-256 before executing the identity probe, so
modified host bytes are never launched. Hashing accepts only a non-empty regular
file up to 512 MiB and fails closed on symlinks or special files instead of
following or blocking on them. The
strict offline contract is
[`desktop-host-doctor.schema.json`](schemas/desktop-host-doctor.schema.json).

Host acquisition never deletes the existing executable before the replacement
has passed the signed digest and exact version-identity checks. Activation uses
a same-directory backup and atomic rename, restores the previous executable if
publication fails, and synchronizes the containing directory on Unix. If the
binary already matches the current signed release, PAM repairs missing or stale
provenance locally without downloading the host again. A later invocation also
recognizes and restores a backup left between the two activation renames.
Provenance replacement follows the same backup/rollback protocol and closes its
temporary handle before activation for Windows compatibility. Per-process
monotonic suffixes prevent an abandoned temporary name from colliding with a
later acquisition, while normal cleanup still removes failed downloads.
The file transaction module has no PHP or third-party dependency and runs as a
native `rustc --test` contract on `windows-2022` in CI, covering replacement,
closed-handle activation, rollback, interrupted-backup recovery and unique
same-directory temporary paths on NTFS semantics.

The aggregation command refuses missing, symbolic-link, or non-portably named artifacts, hashes files by
streaming them in 64 KiB chunks, verifies that they did not change during the
read, and writes `dist/product-release.json` plus its SHA-256 sidecar without
overwriting previous evidence. The manifest uses integer surface codes `1`,
`2`, and `3` and is covered by
[`product-release.schema.json`](schemas/product-release.schema.json).

Before uploading or installing that release, `pam release:verify` validates the
manifest checksum and schema offline, requires Server, Native, and Desktop
artifacts, confines every portable path to the Product workspace, rejects
symbolic links, and streams each artifact again to verify its recorded size and
SHA-256. An optional manifest path verifies a separately staged copy, for
example `pam release:verify dist-copy/product-release.json`.

The generated Desktop application includes a responsive Product Control Center.
It reports only locally verified signals: Server readiness, Desktop worker
availability, contract codes, request latency, sample time, and bounded outbox
occupancy. Native remains visibly “not monitored” instead of receiving a
misleading healthy state. The console is keyboard accessible, uses semantic
live regions and a native `meter`, preserves 48 px controls, and honors reduced
motion. Its PHP worker keeps at most 24 local Server observations in a
versioned, 16 KiB file written through temporary-file replacement. Each sample
contains only integer readiness, latency, and observation time; the accessible
chart always retains exact textual values and a summary.

`pam release --check` at the Product root is also cross-surface: it runs Doctor,
lint, and tests independently in all three applications, then executes the
shared contract suite. It never treats the manifest-only workspace root as an
empty successful PHP project. After those gates pass, `pam release` adds the
final artifact aggregation and verifies the resulting release before reporting
success.

CI and editor integrations can consume project discovery and health without
parsing terminal decoration:

```bash
pam info --json
pam doctor --json
pam doctor --schema
pam doctor --validate doctor-report.json
```

`pam doctor --schema` prints the exact embedded Draft 2020-12 contract without
network access. Schema version `1` keeps the legacy `schema` alias while new
consumers should read `schemaVersion`. Result, project, action, and artifact
variants use sequential integer codes; unknown object fields fail validation.
The schema is also versioned at
[`schemas/doctor-report.schema.json`](schemas/doctor-report.schema.json).
Saved reports can be checked without PHP, network access, or another schema
package using `pam doctor --validate`. The executable gate reads at most 1 MiB,
accepts only regular non-symlink files, rejects unknown fields and integer codes,
and verifies semantic relationships such as health/result/exit consistency,
project identity, and artifact totals.

The main CI job exercises the full producer/consumer path for both a direct PHP
target and a discovered project. After validation it publishes the two reports,
the exact embedded schema, integer-coded provenance, and a self-verified
`SHA256SUMS` for seven days. The workflow definition is not evidence by itself;
the artifact from a successful hosted run is the portable proof.

Legacy namespaced commands such as `pam mobile build` remain supported for CI
and explicit cross-project automation.

## Add ecosystem capabilities

```bash
pam packages
pam add maps
pam add auth
pam remove maps
```

PAM queries package metadata and performs a non-mutating Composer preflight
before it changes the manifest or lockfile.

## Automation

Generated build state stays project-scoped. Use `pam clean --dry-run --json` to
measure reclaimable bytes in CI or `pam clean` for the safe daily tier. Complete
host and Cargo rebuilds can use `pam clean --all`; see
[development artifact retention](development-artifacts.md) for the exact
allowlist and stable integer contract.

Interactive flows always have deterministic command forms for CI:

```bash
pam init my-native-app \
  --template mobile \
  --no-interaction
```

Use `pam help <command>` for focused usage. Commands intended for tooling expose
stable exit codes. `pam info --json` provides the stable project-discovery
payload; generated command documentation is checked in CI so the website and
binary cannot silently drift.
