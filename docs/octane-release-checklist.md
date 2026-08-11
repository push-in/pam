# PAM Octane release checklist

PAM Octane is released from `packages/octane` as the independent Composer
package `pushinbr/pam-octane`. Runtime and package changes are tagged together so
their native contracts cannot drift.

## One-time publication setup

1. Create the public repository `push-in/pam-octane` with `main` as its default
   branch and no generated starter files.
2. Add a repository-scoped deploy key with write access to that repository.
3. Store its private key in the PAM repository Actions secret
   `PAM_OCTANE_DEPLOY_KEY`.
4. Submit `https://github.com/push-in/pam-octane` to Packagist as
   `pushinbr/pam-octane` and enable its GitHub update hook.
5. Enable GitHub Discussions and private vulnerability reporting in `push-in/pam`.
6. Protect `main` with pull-request review and the required CI, PAM Octane,
   Composer-package and security checks. Apply the same rule to the package mirror.
7. Enable Dependabot security updates, secret scanning and push protection where
   the organization plan makes those controls available.

These operations require organization-owner credentials and are intentionally
not performed by repository scripts.

## Every release

1. Replace `Unreleased` in `packages/octane/CHANGELOG.md` with the release date.
2. Ensure the version heading matches `Cargo.toml` and the `vX.Y.Z` tag.
3. Run:

   ```bash
   composer --working-dir=packages/octane verify
   scripts/package-release.sh validate
   cargo fmt --all -- --check
   cargo clippy --locked --all-targets --all-features -- -D warnings
   cargo test --locked --all-targets --all-features -- --test-threads=1
   PAM_SOAK_DURATION=30m benchmarks/octane/soak.sh
   ```

4. Run the benchmark matrix from a clean commit and retain raw results, metadata
   and the soak report as release evidence.
5. Create and push the signed `vX.Y.Z` tag. Wait for runtime release assets and
   attestations to finish.
6. Dispatch `Composer packages` with `publish=true` and verify the immutable
   `push-in/pam-octane` tag.
7. Confirm the new version is installable from Packagist.
8. Dispatch `PAM Octane public release smoke` for the same tag and package version.
9. Test a staging application with its real extensions, database, Redis and
   rollback procedure before announcing general availability.

Do not publish benchmark claims from a dirty worktree or from a run that omits
hardware, versions, worker counts, latency percentiles, errors or memory data.
