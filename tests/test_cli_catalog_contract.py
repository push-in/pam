import json
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


class CliCatalogContractTest(unittest.TestCase):
    def test_schema_and_runtime_authority_stay_aligned(self) -> None:
        schema_path = ROOT / "docs/schemas/cli-catalog.schema.json"
        compatibility_schema_path = ROOT / "docs/schemas/cli-catalog-compatibility.schema.json"
        baseline_path = ROOT / "docs/contracts/cli-catalog-v1.json"
        schema = json.loads(schema_path.read_text(encoding="utf-8"))
        compatibility_schema = json.loads(compatibility_schema_path.read_text(encoding="utf-8"))
        baseline = json.loads(baseline_path.read_text(encoding="utf-8"))
        source = (ROOT / "src/catalog.rs").read_text(encoding="utf-8")
        main = (ROOT / "src/main.rs").read_text(encoding="utf-8")
        docs = (ROOT / "docs/cli.md").read_text(encoding="utf-8")
        workflow = (ROOT / ".github/workflows/ci.yml").read_text(encoding="utf-8")
        self.assertIn('include_str!("../docs/schemas/cli-catalog.schema.json")', source)
        self.assertIn(
            'include_str!("../docs/schemas/cli-catalog-compatibility.schema.json")', source
        )
        self.assertEqual(
            compatibility_schema["$defs"]["change"]["properties"]["changeCode"]["enum"],
            [1, 2, 3],
        )
        self.assertEqual(baseline["schemaVersion"], 1)
        self.assertGreater(len(baseline["commands"]), 0)
        self.assertEqual(
            len({command["name"] for command in baseline["commands"]}),
            len(baseline["commands"]),
        )
        self.assertIn('arguments.as_slice() == ["--schema"]', main)
        self.assertIn('option == "--validate"', main)
        self.assertIn("pam catalog --schema", docs)
        self.assertIn("Produce and consume portable CLI catalog evidence", workflow)
        self.assertIn("./target/debug/pam catalog --validate", workflow)
        self.assertIn("./target/debug/pam catalog --compat", workflow)
        self.assertIn("docs/contracts/cli-catalog-v1.json", workflow)
        self.assertIn("pam-cli-catalog-contract-${{ github.sha }}", workflow)
        self.assertEqual(
            schema["$defs"]["command"]["properties"]["groupCode"]["enum"],
            list(range(1, 10)),
        )
        for code, variant in enumerate(
            [
                "Project",
                "Develop",
                "Generate",
                "Ecosystem",
                "Quality",
                "Ship",
                "Runtime",
                "Observe",
                "Advanced",
            ],
            start=1,
        ):
            self.assertIn(f"{variant} = {code}", source)


if __name__ == "__main__":
    unittest.main()
