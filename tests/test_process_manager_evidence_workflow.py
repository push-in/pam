import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]


class ProcessManagerEvidenceWorkflowTest(unittest.TestCase):
    def test_manager_recovery_suite_is_executable_and_publishable(self) -> None:
        workflow = (ROOT / ".github/workflows/evidence.yml").read_text(
            encoding="utf-8"
        )
        self.assertIn("5 manager recovery", workflow)
        self.assertIn("- '5'", workflow)
        self.assertIn("5) benchmarks/process-manager/run.sh ;;", workflow)
        self.assertIn(
            "5) results=benchmarks/process-manager/results ;;", workflow
        )
        self.assertIn("PAM_RECOVERY_ROUNDS: '10'", workflow)
        self.assertIn("PAM_RECOVERY_MAX_P95_MILLIS: '300'", workflow)
        self.assertIn(
            "PAM_RECOVERY_MAX_RSS_GROWTH_BYTES: '16777216'", workflow
        )
        self.assertIn("benchmarks/process-manager/results/", workflow)
        self.assertIn('"${results}" "${PAM_EVIDENCE_SUITE_ID}" --verify', workflow)
        self.assertIn("if-no-files-found: error", workflow)
        self.assertIn("retention-days: 30", workflow)
        self.assertNotIn("continue-on-error", workflow)
        self.assertEqual(workflow.count("PAM_EVIDENCE_SUITE_ID == '6'"), 5)
        self.assertIn(
            "composer install --working-dir=compat/composer-smoke --dry-run --no-dev",
            workflow,
        )
        self.assertIn(
            "composer install --working-dir=compat/composer-smoke --no-dev",
            workflow,
        )
        self.assertIn("6 PM2 recovery comparison", workflow)
        self.assertIn("- '6'", workflow)
        self.assertIn("node-version: '22.22.0'", workflow)
        self.assertIn("npm ci --dry-run --ignore-scripts", workflow)
        self.assertIn("npm audit --omit=dev --audit-level=high", workflow)
        self.assertIn("npm ci --ignore-scripts", workflow)
        self.assertIn("6) benchmarks/process-manager/compare-pm2.sh ;;", workflow)
        self.assertIn(
            "6) results=benchmarks/process-manager/results/comparison ;;", workflow
        )

        harness = (ROOT / "benchmarks/process-manager/run.sh").read_text(
            encoding="utf-8"
        )
        comparison = (ROOT / "benchmarks/process-manager/compare-pm2.sh").read_text(
            encoding="utf-8"
        )
        for script in (harness, comparison):
            self.assertIn("diff --quiet --ignore-submodules HEAD --", script)
            self.assertIn('"dirty_scope":"tracked_files"', script)
            self.assertIn('"pam_native_commit":"%s"', script)
        self.assertNotIn("status --porcelain", harness)
        self.assertNotIn("status --porcelain", comparison)
        self.assertIn('tail -c 65536 >"${results}/launch-error.log"', harness)
        self.assertIn('tail -c 1048576 "${application_error}"', harness)
        self.assertIn("launch_status", harness)


if __name__ == "__main__":
    unittest.main()
