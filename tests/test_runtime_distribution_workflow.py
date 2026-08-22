import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]


class RuntimeDistributionWorkflowTest(unittest.TestCase):
    def test_runtime_launchers_expose_the_canonical_package_home(self) -> None:
        linux = (ROOT / "scripts/package-runtime.sh").read_text(encoding="utf-8")
        macos = (ROOT / "scripts/package-runtime-macos.sh").read_text(encoding="utf-8")

        self.assertIn('export PAM_HOME="$PAM_INSTALL_ROOT/share/pam"', linux)
        self.assertIn('export PAM_HOME="${pam_install_root}/share/pam"', macos)

    def test_clean_host_workflow_is_signed_bounded_and_fail_closed(self) -> None:
        workflow = (ROOT / ".github/workflows/runtime-distribution.yml").read_text(
            encoding="utf-8"
        )

        self.assertIn("workflow_call:", workflow)
        self.assertIn("evidence_key:", workflow)
        self.assertIn("evidence_key_identity:", workflow)
        self.assertIn("required: true", workflow)
        self.assertIn("target: linux-x86_64", workflow)
        self.assertIn("target: linux-aarch64", workflow)
        self.assertIn("runner: ubuntu-24.04-arm", workflow)
        self.assertIn("target: macos-x86_64", workflow)
        self.assertIn("target: macos-arm64", workflow)
        self.assertIn("runner: macos-15-intel", workflow)
        self.assertIn("runner: macos-15", workflow)
        self.assertIn("architecture_code: 1", workflow)
        self.assertIn("architecture_code: 2", workflow)
        self.assertIn("name: pam-${{ matrix.target }}", workflow)
        self.assertIn("gh release download", workflow)
        self.assertEqual(workflow.count("gh attestation verify"), 4)
        self.assertEqual(workflow.count("--signer-workflow"), 4)
        self.assertEqual(workflow.count("--source-digest"), 4)
        self.assertEqual(workflow.count("--deny-self-hosted-runners"), 4)
        self.assertEqual(workflow.count("gh attestation download"), 4)
        self.assertIn("gh attestation trusted-root", workflow)
        self.assertIn("candidate-verification.json", workflow)
        self.assertIn("baseline-verification.json", workflow)
        self.assertEqual(
            workflow.count(
                'tagged_revision=$(git rev-parse "refs/tags/${PAM_CURRENT_TAG}^{commit}")'
            ),
            2,
        )
        self.assertEqual(
            workflow.count('test "${current_revision}" = "${tagged_revision}"'),
            2,
        )
        self.assertEqual(
            workflow.count(
                "printf 'PAM_CURRENT_SOURCE_REF=refs/tags/%s\\n' \"${PAM_CURRENT_TAG}\""
            ),
            2,
        )
        self.assertEqual(
            workflow.count('--source-ref "${PAM_CURRENT_SOURCE_REF}"'), 2
        )
        self.assertNotIn(
            'test "${GITHUB_REF}" = "refs/tags/${PAM_CURRENT_TAG}"', workflow
        )
        self.assertNotIn('test "${current_revision}" = "${GITHUB_SHA}"', workflow)
        self.assertIn('--pattern "pam-${baseline_tag}-${{ matrix.target }}.tar.gz"', workflow)
        self.assertEqual(
            workflow.count("python3 scripts/select-release-baseline.py"), 2
        )
        self.assertNotIn("sort -V", workflow)
        self.assertIn("--network none", workflow)
        self.assertIn("docker image inspect", workflow)
        self.assertIn("baselineArtifact:", workflow)
        self.assertIn("git rev-parse HEAD", workflow)
        self.assertIn("distribution:sign", workflow)
        self.assertIn("distribution:verify", workflow)
        self.assertEqual(workflow.count('--arg releaseVersion "${PAM_CURRENT_TAG}"'), 2)
        self.assertEqual(workflow.count("releaseVersion: $releaseVersion"), 2)
        self.assertEqual(workflow.count("issued_at_unix=$(date +%s)"), 2)
        self.assertEqual(
            workflow.count("expires_at_unix=$((issued_at_unix + 2678400))"), 2
        )
        self.assertEqual(workflow.count("issuedAtUnix: $issuedAtUnix"), 2)
        self.assertEqual(workflow.count("expiresAtUnix: $expiresAtUnix"), 2)
        self.assertIn("shred -u", workflow)
        self.assertIn(".signingIdentitySha256", workflow)
        self.assertNotIn("continue-on-error", workflow)
        self.assertIn("if-no-files-found: error", workflow)
        self.assertIn("retention-days: 30", workflow)
        self.assertIn("certification-runtime-distribution-${{ matrix.target }}", workflow)
        self.assertIn('test "${machine}" = "${PAM_EXPECTED_MACHINE}"', workflow)
        self.assertIn('test -n "${ImageVersion:-}"', workflow)
        self.assertIn("platformCode: 2", workflow)
        self.assertIn("mv -fh clean-host/install/next", workflow)
        self.assertEqual(workflow.count('"${candidate_root}/bin/pam-run" -m'), 2)
        self.assertEqual(workflow.count("module-load.stderr"), 4)
        self.assertEqual(workflow.count("runtime-loaded-modules.txt"), 6)
        self.assertEqual(
            workflow.count('test "$(wc -c <clean-host/loaded-modules.unsorted)" -le 1048576'),
            2,
        )

    def test_release_cannot_publish_before_distribution_certification(self) -> None:
        workflow = (ROOT / ".github/workflows/release.yml").read_text(
            encoding="utf-8"
        )

        self.assertIn("uses: ./.github/workflows/runtime-distribution.yml", workflow)
        self.assertIn("release-preflight:", workflow)
        self.assertIn("Validate release authority before expensive builds", workflow)
        self.assertEqual(workflow.count("needs: release-preflight"), 4)
        self.assertIn('test -n "${PAM_EVIDENCE_KEY}"', workflow)
        self.assertIn(
            'tagged_revision=$(git rev-parse "refs/tags/${PAM_RELEASE_TAG}^{commit}")',
            workflow,
        )
        self.assertIn(
            "evidence_key: ${{ secrets.PAM_DISTRIBUTION_EVIDENCE_KEY }}", workflow
        )
        self.assertIn("vars.PAM_DISTRIBUTION_EVIDENCE_KEY_SHA256", workflow)
        self.assertIn("certify-runtime-distribution]", workflow)
        self.assertIn("docs/schemas/distribution-evidence.schema.json", workflow)
        self.assertIn("Package the signed clean-host certification", workflow)
        self.assertIn(
            "for target in linux-x86_64 linux-aarch64 macos-x86_64 macos-arm64",
            workflow,
        )
        self.assertIn("runtime-distribution-${target}.tar.gz", workflow)
        self.assertIn("PAM_UPDATE_SIGNING_IDENTITY_SHA256", workflow)
        self.assertIn("PAM_UPDATE_NEXT_SIGNING_IDENTITY_SHA256", workflow)
        self.assertIn("PAM_DISTRIBUTION_NEXT_EVIDENCE_KEY_SHA256", workflow)
        self.assertIn("pam-${PAM_RELEASE_TAG}-${target}.update.json", workflow)
        self.assertEqual(workflow.count("grep -Eq '^[0-9a-f]{64}$'"), 6)
        self.assertEqual(
            workflow.count(
                'test "${PAM_UPDATE_NEXT_SIGNING_IDENTITY_SHA256}" != "${PAM_UPDATE_SIGNING_IDENTITY_SHA256}"'
            ),
            2,
        )
        self.assertEqual(
            workflow.count("certification-runtime-distribution-linux-"), 2
        )
        self.assertEqual(
            workflow.count("certification-runtime-distribution-macos-"), 2
        )


if __name__ == "__main__":
    unittest.main()
