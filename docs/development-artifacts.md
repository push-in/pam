# Development artifact retention

PAM keeps generated development state inside the active project. Inspect what
can be reclaimed before deleting anything:

```bash
pam clean --dry-run
pam clean --dry-run --json
```

`pam info --json` exposes the complete reclaimable total as
`artifactFootprint`, with project-relative entries for generated Android and
iOS hosts plus the Cargo target. The older `developmentArtifacts` field remains
available for consumers that specifically measure `.pam-native`. Human
`pam info` output uses the complete footprint, so Runtime and Desktop projects
do not report zero while their Cargo build output is consuming disk.
The adjacent `artifactBudget` object reports `limitBytes`, the exact cleanup
command and an integer `stateCode`: `1` within budget, `2` exceeded, and `3`
incomplete scan. These values are backed by the Runtime's
`ArtifactBudgetState` enum rather than string statuses.

Contextual Native development also performs bounded cleanup before rebuilding.
Android removes only app/root build outputs and its project-local Gradle caches;
iOS removes Xcode's actual `.pam-native/ios/App/DerivedData` directory. Both
paths preserve generated host sources, screenshots and release evidence.

Every contextual `pam dev` session enforces an 8 GiB project-local ceiling
before launch. When the dev process exits, PAM **always** performs the same
scoped complete cleanup as `pam clean --all`, regardless of artifact size and
after both successful and failed Runtime, Native and Desktop sessions. An
original dev failure remains the reported failure if cleanup also cannot run.
This mandatory post-flight rule leaves no regenerable Android, iOS, Gradle or
Cargo build garbage behind. The ceiling can only be reduced—not expanded—by setting
`PAM_DEV_ARTIFACT_BUDGET_BYTES` to an integer from 256 MiB through 8 GiB. An
incomplete scan, symlink or unexpected non-directory fails closed instead of
deleting an uncertain path.

Cleanup never removes `.pam` as a whole. It targets only known cache children,
preserving Composer lock state, package-owned host provenance, Firebase
configuration and other project authority stored beside those caches.

The default cleanup removes only regenerable build outputs and caches from the
generated Android/iOS hosts, `.pam/cache`, `.pam/phpunit-cache`, plus Cargo
incremental directories. For framework source workspaces it also covers fixed,
project-relative Android Gradle/Kotlin caches and module builds, SwiftPM's
root `.build` or PAM Native's `ios/.build`, Python bytecode directories used by
repository verification, and the root caches of pytest, mypy and Ruff. It
never recursively guesses cache names. PAM Product applies the same
root-tooling allowlist in addition to its three application-specific targets,
so monorepo verification cannot escape the shared footprint or budget. Cleanup
does not
touch source code, `vendor`, application data, databases, sessions, `dist`,
screenshots, release evidence, or user-level caches.

PAM deliberately preserves shared user-level Android SDK, NDK, emulator and
Composer download caches. They are reusable tool installations, not outputs of
one project build; deleting them after every command would waste bandwidth and
make community startup slower. Project-local Gradle caches and build trees are
still removed unconditionally.

The repository-wide build hygiene contract is mandatory: every local or CI
build must clean all regenerable intermediates on exit. Packaging commands must
first copy the declared APK, AAB, IPA, archive or binary into `dist`, and then
remove their build trees. Only those final deliverables and explicit evidence
may be retained.

```bash
pam clean
```

Use the complete tier when a project needs a clean rebuild or local disk usage
has grown too far:

```bash
pam clean --all
```

`--all` additionally removes the generated `.pam-native/android` and
`.pam-native/ios` hosts and the project-local Cargo `target`. All three are
recreated by normal prepare/build commands. PAM resolves the project root first,
uses a fixed allowlist, refuses artifact roots that are symlinks or files, and
never follows a path outside the project.

The command also accepts the root of a Rust workspace containing `Cargo.toml`.
This keeps development of PAM Runtime, PAM Native and PAM Desktop under the same
retention contract as applications built with them.

Every existing allowlisted directory must resolve canonically to its exact
project-relative path. A symlink at the target or in any ancestor makes the
footprint incomplete, blocks automatic budget cleanup and makes explicit
cleanup fail before deletion. Pointing `android`, `ios`, `scripts` or another
ancestor at an external directory can therefore never redirect PAM's remover.
Symlinks or unsupported entries inside an artifact directory also make its
scan incomplete. A dry-run reports the partial measurement, while an actual
cleanup refuses to delete it until the ambiguous entry is removed.
Validation and measurement cover every selected target before the first
deletion begins, preventing a later invalid target from causing a predictable
partial cleanup. Each path is rechecked immediately before removal so a changed
file type or symlink is rejected rather than followed.

Ordinary runtime CI builds and smoke-tests the release binary on an ephemeral
runner but does not upload that disposable binary. Versioned distribution stays
in the release workflow, preventing every source push from retaining a duplicate
multi-megabyte executable for the repository's default artifact lifetime.

GitHub Actions uses the same bounded-retention principle. Cross-job build
prerequisites expire after 1 day, ordinary CI diagnostics and intermediate
release archives after 7 days, and reproducible benchmark or compatibility
evidence after 30 days. Published GitHub Release assets remain the durable
distribution channel. `scripts/check-artifact-retention.py` rejects workflow
uploads without an explicit lifetime or with retention above 30 days.

Every repository CI build also ends with the project-scoped
`scripts/cleanup-build-artifacts.sh` under `if: always()`, after any declared
artifact upload. This makes the local `pam dev` rule and hosted runner rule the
same: no regenerable Cargo, Gradle or Xcode build tree survives the job.

The JSON contract uses `schemaVersion: 1`, `resultCode: 1`, project type codes
from the public PAM project enum, operation codes `1` (preview), `2` (default
cleanup), and `3` (complete cleanup), plus artifact kind codes `1` (cache) and
`2` (build). Paths in individual entries are project-relative so CI reports do
not expose machine-specific absolute artifact locations.
