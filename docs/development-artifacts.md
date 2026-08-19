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

Contextual Native development also performs bounded cleanup before rebuilding.
Android removes only app/root build outputs and its project-local Gradle caches;
iOS removes Xcode's actual `.pam-native/ios/App/DerivedData` directory. Both
paths preserve generated host sources, screenshots and release evidence.

The default cleanup removes only regenerable build outputs and caches from the
generated Android/iOS hosts plus Cargo incremental directories. It does not
touch source code, `vendor`, application data, databases, sessions, `dist`,
screenshots, release evidence, or user-level caches.

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

Ordinary runtime CI builds and smoke-tests the release binary on an ephemeral
runner but does not upload that disposable binary. Versioned distribution stays
in the release workflow, preventing every source push from retaining a duplicate
multi-megabyte executable for the repository's default artifact lifetime.

The JSON contract uses `schemaVersion: 1`, `resultCode: 1`, project type codes
from the public PAM project enum, operation codes `1` (preview), `2` (default
cleanup), and `3` (complete cleanup), plus artifact kind codes `1` (cache) and
`2` (build). Paths in individual entries are project-relative so CI reports do
not expose machine-specific absolute artifact locations.
