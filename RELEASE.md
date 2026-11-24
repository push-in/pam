# Release checklist

How a PAM release goes out, including the steps that are easy to miss. The
package model is in [docs/packages.md](docs/packages.md); the Octane bridge has
its own [checklist](docs/octane-release-checklist.md).

## 1. Release PR

- [ ] `Cargo.toml` and `Cargo.lock` carry the new PAM version, and the root
      `CHANGELOG.md` has a dated `## X.Y.Z - YYYY-MM-DD` heading
      (`scripts/package-release.sh validate-tag vX.Y.Z` checks both).
- [ ] Every package that changed has a dated `## X.Y.Z - YYYY-MM-DD` heading in
      `packages/<dir>/CHANGELOG.md`, and the packages that depend on it require
      the new version (for example the skeleton's `pushinbr/nimbus` constraint).
- [ ] The local path repository versions follow those headings by themselves:
      `build.rs` reads the first released heading of each package changelog
      into `local_package_versions.rs`, and `src/commands.rs` offers
      `packages/*` to new projects at those versions. Do not edit versions by
      hand there; the test `local_package_versions_follow_each_changelog` fails
      when a changelog and the generated map disagree.
- [ ] New files in a package are in the split allowlist (`expected_files` in
      `verify_split`, `scripts/package-release.sh`). A file outside it fails
      **Verify <package>** on the tag, after the release is already built.
      Run `scripts/package-release.sh split <name> HEAD` and
      `scripts/package-release.sh verify-split <name> <sha>` locally for every
      package with a new top-level file or directory.
- [ ] Optional: bump `ARG PAM_VERSION` in `packages/skeleton/Dockerfile` so pull
      request smoke builds of the image use the new runtime.
- [ ] `scripts/package-release.sh validate` and
      `scripts/community-experience-gate.sh all` pass (see CONTRIBUTING.md).

## 2. Tag

Push the `vX.Y.Z` tag from the merged release commit on `main`.

- **Release** (`release.yml`) builds every target, runs the release gates,
  creates the GitHub release, then calls **Nimbus image and Helm chart**
  (`skeleton-image.yml`) to push `ghcr.io/push-in/nimbus-skeleton:X.Y.Z` and
  `:latest` (amd64 + arm64, SBOM and provenance). A release created with
  `GITHUB_TOKEN` never fires `release: published`, which is why the image is a
  called workflow and not an event listener. To rebuild an image by hand:
  `gh workflow run skeleton-image.yml -R push-in/pam -f pam_version=vX.Y.Z -f push=true`.
  Rebuilding an older tag also moves `:latest`, so only do it for the newest
  release.
- **Composer packages** (`composer-packages.yml`) verifies every split on the
  tag push. It does not publish on its own.

## 3. Mirrors and Packagist

- [ ] After the GitHub release exists, publish the splits:
      `gh workflow run composer-packages.yml -R push-in/pam -f release_tag=vX.Y.Z -f publish=true`.
      Packages released on their own use **Independent Composer package release**
      (`composer-package-release.yml`), in dependency order.
- [ ] Check that Packagist shows the new tags (`composer show -a pushinbr/<name>`);
      trigger an update from the Packagist package page if a webhook missed it.
- [ ] Run a public Composer dry run of the skeleton
      (`composer create-project pushinbr/nimbus-skeleton /tmp/s --no-install`
      followed by `composer update --dry-run`) before announcing the release.

## Security settings on the mirrors

Mirrors are pure split outputs (the `main` branch is fast-forwarded to the
split commit), so workflows and `SECURITY.md` live in `packages/<dir>/` in this
repository. Dependabot alerts, secret scanning with push protection and private
vulnerability reporting are enabled on each mirror; reports are redirected to
`push-in/pam` by each package's `SECURITY.md`.
