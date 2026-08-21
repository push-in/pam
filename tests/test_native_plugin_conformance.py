import json
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
KIT = ROOT / "ecosystem/pam-native-plugin-kit"


class NativePluginConformanceTest(unittest.TestCase):
    def test_portable_conformance_contract_is_bounded_and_ci_enforced(self) -> None:
        schema = json.loads(
            (KIT / "resources/pam-native-conformance.schema.json").read_text(
                encoding="utf-8"
            )
        )
        runner = (KIT / "src/ConformanceRunner.php").read_text(encoding="utf-8")
        command = (KIT / "bin/pam-native-plugin").read_text(encoding="utf-8")
        workflow = (KIT / ".github/workflows/ci.yml").read_text(encoding="utf-8")

        self.assertEqual(schema["properties"]["schemaVersion"]["const"], 1)
        self.assertEqual(schema["properties"]["surfaceCode"]["const"], 2)
        self.assertEqual(schema["properties"]["resultCode"]["enum"], [1, 2])
        self.assertEqual(
            schema["properties"]["checks"]["items"]["properties"]["checkCode"]["enum"],
            list(range(1, 8)),
        )
        self.assertIn("MAX_SOURCE_FILES = 256", runner)
        self.assertIn("MAX_SOURCE_BYTES = 16_777_216", runner)
        self.assertIn("is_link", runner)
        self.assertIn("realpath", runner)
        self.assertIn("DeterministicGeneration", runner)
        self.assertIn("conformance <plugin-directory> [--json]", command)
        self.assertIn("Certify a generated contributor plugin", workflow)
        self.assertIn("pam-native-conformance.schema.json", workflow)


if __name__ == "__main__":
    unittest.main()
