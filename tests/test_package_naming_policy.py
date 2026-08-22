import json
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]


class PackageNamingPolicyTest(unittest.TestCase):
    def test_http_extensions_use_the_http_product_family(self) -> None:
        catalog = json.loads(
            (ROOT / "packages/packages.json").read_text(encoding="utf-8")
        )
        packages = {package["name"]: package for package in catalog["packages"]}

        for name in ("pushinbr/pam-http-psr", "pushinbr/pam-http-testing"):
            self.assertIn(name, packages)
            suffix = name.removeprefix("pushinbr/")
            self.assertEqual(packages[name]["repository"], suffix)
            self.assertEqual(packages[name]["path"], f"packages/{suffix.removeprefix('pam-')}")

    def test_old_generic_names_are_abandoned_compatibility_packages(self) -> None:
        migrations = {
            "psr": "pushinbr/pam-http-psr",
            "testing": "pushinbr/pam-http-testing",
        }

        for directory, replacement in migrations.items():
            manifest = json.loads(
                (ROOT / "packages" / directory / "composer.json").read_text(
                    encoding="utf-8"
                )
            )
            self.assertEqual(manifest["type"], "metapackage")
            self.assertEqual(manifest["abandoned"], replacement)
            self.assertIn(replacement, manifest["require"])

    def test_new_project_template_uses_only_product_owned_names(self) -> None:
        commands = (ROOT / "src/commands.rs").read_text(encoding="utf-8")
        skeleton = json.loads(
            (ROOT / "packages/skeleton/composer.json").read_text(encoding="utf-8")
        )

        self.assertNotIn('"pushinbr/pam-testing"', commands)
        self.assertIn("pushinbr/pam-http-testing", skeleton["require-dev"])
        self.assertIn("pushinbr/pam-http-psr", skeleton["require"])


if __name__ == "__main__":
    unittest.main()
