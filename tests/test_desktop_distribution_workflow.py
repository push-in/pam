import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]


class DesktopDistributionWorkflowTest(unittest.TestCase):
    def test_linux_host_certification_is_immutable_offline_and_signed(self) -> None:
        workflow = (
            ROOT / ".github/workflows/desktop-linux-distribution.yml"
        ).read_text(encoding="utf-8")

        self.assertIn("workflow_call:", workflow)
        self.assertIn("workflow_dispatch:", workflow)
        self.assertIn("evidence_key:", workflow)
        self.assertIn("required: true", workflow)
        self.assertIn("PAM_DESKTOP_REPOSITORY: push-in/pam-desktop", workflow)
        self.assertIn("git -C desktop-current-source describe --tags --exact-match", workflow)
        self.assertIn("git -C desktop-baseline-source describe --tags --exact-match", workflow)
        self.assertEqual(workflow.count(".tar.gz.sha256"), 2)
        self.assertIn(
            'desktop-current-source/scripts/test-host-archive.sh "${candidate_archive}"',
            workflow,
        )
        self.assertIn(
            'desktop-baseline-source/scripts/test-host-archive.sh "${baseline_archive}"',
            workflow,
        )
        self.assertIn("candidate-release.sha256", workflow)
        self.assertIn("baseline-release.sha256", workflow)
        self.assertEqual(workflow.count("gh attestation verify"), 1)
        self.assertIn("for role in candidate baseline", workflow)
        self.assertIn("--source-ref", workflow)
        self.assertIn("--source-digest", workflow)
        self.assertIn("--deny-self-hosted-runners", workflow)
        self.assertIn("gh attestation trusted-root", workflow)
        self.assertIn("docker run --rm --network none", workflow)
        self.assertIn("ubuntu:22.04", workflow)
        self.assertIn('"${baseline_root}/install.sh"', workflow)
        self.assertIn('"${candidate_root}/install.sh"', workflow)
        self.assertIn('ldd "${candidate_root}/bin/pam-desktop"', workflow)
        self.assertIn('! grep -F "not found" "${dependency_inventory}"', workflow)
        self.assertIn("dependency-inventory.txt", workflow)
        self.assertNotIn("dependency-inventory.sha256", workflow)
        self.assertIn("mv -Tf", workflow)
        self.assertIn("surfaceCode: 3", workflow)
        self.assertIn("packageCode: 1", workflow)
        self.assertNotIn("platformVerification:", workflow)
        for check_code in range(1, 8):
            self.assertIn(f"checkCode: {check_code}", workflow)
        self.assertIn("distribution:sign", workflow)
        self.assertIn("distribution:verify", workflow)
        self.assertIn("PAM_EXPECTED_EVIDENCE_IDENTITY", workflow)
        self.assertIn("platformVerificationSha256", workflow)
        self.assertIn("if-no-files-found: error", workflow)
        self.assertIn("retention-days: 30", workflow)
        self.assertNotIn("continue-on-error", workflow)


if __name__ == "__main__":
    unittest.main()
