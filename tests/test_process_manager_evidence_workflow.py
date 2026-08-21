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
        self.assertIn("PAM_RECOVERY_MAX_P95_MILLIS: '2000'", workflow)
        self.assertIn(
            "PAM_RECOVERY_MAX_RSS_GROWTH_BYTES: '16777216'", workflow
        )
        self.assertIn("benchmarks/process-manager/results/", workflow)
        self.assertIn('"${results}" "${PAM_EVIDENCE_SUITE_ID}" --verify', workflow)
        self.assertIn("if-no-files-found: error", workflow)
        self.assertIn("retention-days: 30", workflow)
        self.assertNotIn("continue-on-error", workflow)

        harness = (ROOT / "benchmarks/process-manager/run.sh").read_text(
            encoding="utf-8"
        )
        self.assertIn('tail -c 65536 >"${results}/launch-error.log"', harness)
        self.assertIn('tail -c 1048576 "${application_error}"', harness)
        self.assertIn("launch_status", harness)


if __name__ == "__main__":
    unittest.main()
