import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]


class DoctorWorkflowContractTest(unittest.TestCase):
    def test_ci_produces_validates_and_seals_both_report_shapes(self) -> None:
        workflow = (ROOT / ".github/workflows/ci.yml").read_text(encoding="utf-8")

        self.assertIn("docs/schemas/doctor-report.schema.json", workflow)
        self.assertIn("tests.test_doctor_workflow", workflow)
        self.assertEqual(workflow.count("pam doctor --validate"), 2)
        self.assertIn("pam-doctor-project/pam.json", workflow)
        self.assertIn('>"${PAM_DOCTOR_EVIDENCE}/runtime.json"', workflow)
        self.assertIn('>"${PAM_DOCTOR_EVIDENCE}/project.json"', workflow)
        self.assertIn('>"${PAM_DOCTOR_EVIDENCE}/schema.json"', workflow)
        self.assertIn("sha256sum --check --strict SHA256SUMS", workflow)
        self.assertIn("pam-doctor-contract-${{ github.sha }}", workflow)
        self.assertIn("if-no-files-found: error", workflow)
        self.assertIn("retention-days: 7", workflow)
        self.assertNotIn("continue-on-error", workflow)


if __name__ == "__main__":
    unittest.main()
