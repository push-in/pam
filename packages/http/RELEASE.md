# Releasing PAM API

PAM API versions independently from the native PAM runtime. A package release
must come from a commit already integrated into the monorepo `main` branch.

## Release gate

1. Date the version entry in `CHANGELOG.md` and complete `UPGRADE.md` for a
   major release.
2. Run `composer verify` in `packages/http`.
3. Run the package metadata and isolated split gates:

   ```bash
   scripts/package-release.sh validate
   scripts/package-release.sh validate-package-tag pushinbr/pam-http v2.0.0
   split_sha=$(scripts/package-release.sh split pushinbr/pam-http HEAD)
   scripts/package-release.sh verify-split pushinbr/pam-http "$split_sha"
   ```

4. Merge through the protected `main` branch. Do not release a pull-request
   commit.
5. Dispatch **Independent Composer package release** with package
   `pushinbr/pam-http`, the exact version tag, `source_ref=main`, and
   `publish=false`. Inspect the retained provenance artifact.
6. Re-run with `publish=true`. The workflow uses only the API mirror deploy
   key, refuses a conflicting existing tag, waits for Packagist and performs a
   dry-run followed by a real install from public distribution.
7. Release `pushinbr/pam-skeleton` only after its exact API dependency is public
   and independently installable.

A failed public-install job means the release is not complete even when the Git
tag exists. Repair the Packagist hook or package metadata and re-run the same
idempotent workflow; never move or replace an existing release tag.
