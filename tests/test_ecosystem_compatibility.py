import json
import subprocess
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parent.parent
SCRIPT = ROOT / "scripts" / "ecosystem-compatibility.py"


class EcosystemCompatibilityTests(unittest.TestCase):
    def run_script(self, *arguments: str, stdin: str = "") -> subprocess.CompletedProcess[str]:
        return subprocess.run(
            [str(SCRIPT), *arguments],
            cwd=ROOT,
            input=stdin,
            text=True,
            capture_output=True,
            check=False,
        )

    def test_matrix_is_unique_and_uses_sequential_role_codes(self) -> None:
        result = self.run_script("matrix")
        self.assertEqual(result.returncode, 0, result.stderr)
        packages = json.loads(result.stdout)
        repositories = [package["repository"] for package in packages]
        self.assertEqual(len(repositories), 26)
        self.assertEqual(len(repositories), len(set(repositories)))
        self.assertEqual({package["roleCode"] for package in packages}, {1, 2, 3, 4})

    def test_inventory_fails_closed_for_an_unregistered_repository(self) -> None:
        matrix = json.loads(self.run_script("matrix").stdout)
        inventory = "\n".join(package["repository"] for package in matrix)
        result = self.run_script("inventory", stdin=f"{inventory}\npam-native-forgotten\n")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("pam-native-forgotten", result.stderr)

    def test_checkout_contract_requires_test_and_native_constraints(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            manifest = {
                "name": "pushinbr/pam-native-auth",
                "require": {"php": "^8.4"},
                "scripts": {},
            }
            Path(directory, "composer.json").write_text(json.dumps(manifest), encoding="utf-8")
            result = self.run_script("verify", directory, "pam-native-auth")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("must constrain pushinbr/pam-native", result.stderr)

    def test_reusable_workflow_certifies_the_calling_package_tag(self) -> None:
        workflow = (ROOT / ".github/workflows/ecosystem-compatibility.yml").read_text(
            encoding="utf-8"
        )
        self.assertIn("workflow_call:", workflow)
        self.assertIn("repository: push-in/pam", workflow)
        self.assertIn("github.event.repository.name == matrix.package.repository", workflow)
        self.assertIn("github.ref_type == 'tag'", workflow)


if __name__ == "__main__":
    unittest.main()
