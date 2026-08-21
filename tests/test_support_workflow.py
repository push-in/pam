import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]


class SupportWorkflowTest(unittest.TestCase):
    def test_ci_certifies_private_bounded_product_and_runtime_reports(self) -> None:
        workflow = (ROOT / ".github/workflows/ci.yml").read_text(encoding="utf-8")

        self.assertIn("pam-support-private-project", workflow)
        self.assertIn("--template product --no-install --no-interaction", workflow)
        self.assertIn("source-file-secret-must-not-leak", workflow)
        self.assertIn("environment-secret-must-not-leak", workflow)
        self.assertIn("support-product.json", workflow)
        self.assertIn("support-runtime.json", workflow)
        self.assertIn('test "$(stat -c %a', workflow)
        self.assertIn("len(raw) <= 512 * 1024", workflow)
        self.assertIn("hashlib.sha256(canonical).hexdigest()", workflow)
        self.assertIn("cannot create new support report", workflow)
        self.assertIn("support-product.json \\", workflow)
        self.assertNotIn("actions/upload-artifact@v5", workflow)


if __name__ == "__main__":
    unittest.main()
