import json
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]


class DesktopHostDoctorContractTest(unittest.TestCase):
    def test_schema_source_docs_and_ci_stay_aligned(self) -> None:
        schema = json.loads(
            (ROOT / "docs/schemas/desktop-host-doctor.schema.json").read_text(
                encoding="utf-8"
            )
        )
        source = (ROOT / "src/desktop.rs").read_text(encoding="utf-8")
        source += (ROOT / "src/desktop_transaction.rs").read_text(encoding="utf-8")
        docs = (ROOT / "docs/getting-started.md").read_text(encoding="utf-8")
        workflow = (ROOT / ".github/workflows/ci.yml").read_text(encoding="utf-8")

        self.assertEqual(schema["properties"]["schemaVersion"]["const"], 1)
        self.assertEqual(schema["properties"]["surfaceCode"]["const"], 3)
        self.assertEqual(schema["properties"]["resultCode"]["enum"], [1, 2])
        self.assertEqual(schema["properties"]["sourceCode"]["enum"], [1, 2, 3, 4])
        self.assertIn('argument == "host:doctor"', source)
        self.assertIn("publish_verified_binary(&temporary, destination)", source)
        self.assertIn("cannot preserve previous {label}", source)
        self.assertIn("rollback failed", source)
        self.assertIn("recovers_an_interrupted_desktop_host_activation", source)
        self.assertIn("replaces_desktop_host_provenance_transactionally", source)
        self.assertIn("TEMPORARY_SEQUENCE", source)
        self.assertIn("pam desktop host:doctor . --json", docs)
        self.assertIn("never deletes the existing executable", docs)
        self.assertIn("desktop-host-doctor.schema.json", workflow)
        self.assertIn("windows-desktop-contracts:", workflow)
        self.assertIn("runs-on: windows-2022", workflow)
        self.assertIn(
            "rustc --edition=2024 -D warnings --test src/desktop_transaction.rs",
            workflow,
        )


if __name__ == "__main__":
    unittest.main()
