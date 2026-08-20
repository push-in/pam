import json
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


class ControlPlaneDiagnosticsContractTest(unittest.TestCase):
    def test_schema_runtime_cli_and_docs_stay_aligned(self) -> None:
        diagnostics = json.loads(
            (ROOT / "docs/schemas/control-plane-diagnostics.schema.json").read_text(
                encoding="utf-8"
            )
        )
        health = json.loads(
            (ROOT / "docs/schemas/control-plane-health.schema.json").read_text(
                encoding="utf-8"
            )
        )
        top = json.loads(
            (ROOT / "docs/schemas/top-sample.schema.json").read_text(encoding="utf-8")
        )
        control = (ROOT / "src/control_plane.rs").read_text(encoding="utf-8")
        worker_state = (ROOT / "src/worker_state.rs").read_text(encoding="utf-8")
        commands = (ROOT / "src/commands.rs").read_text(encoding="utf-8")
        main = (ROOT / "src/main.rs").read_text(encoding="utf-8")
        docs = (ROOT / "docs/production.md").read_text(encoding="utf-8")

        self.assertEqual(diagnostics["properties"]["schemaVersion"]["const"], 1)
        self.assertEqual(diagnostics["properties"]["surfaceCode"]["const"], 1)
        self.assertEqual(diagnostics["properties"]["resultCode"]["enum"], [1, 2])
        self.assertEqual(health["properties"]["schemaVersion"]["const"], 1)
        self.assertEqual(health["properties"]["surfaceCode"]["const"], 1)
        self.assertEqual(health["properties"]["resultCode"]["enum"], [1, 2])
        self.assertEqual(
            health["$defs"]["worker"]["properties"]["state"]["enum"],
            [1, 2, 3, 4],
        )
        self.assertEqual(
            diagnostics["$defs"]["worker"]["properties"]["lifecycleCode"]["enum"],
            [1, 2, 3, 4],
        )
        self.assertEqual(top["properties"]["resultCode"]["enum"], [1, 2])
        self.assertEqual(
            diagnostics["$defs"]["worker"]["properties"]["resultCode"]["enum"],
            [1, 2],
        )
        self.assertEqual(top["properties"]["diagnostics"]["$ref"], "control-plane-diagnostics.schema.json")
        self.assertIn('"/diagnostics"', control)
        self.assertIn('WWW-Authenticate: Bearer realm=\\"pam-control-plane\\"', control)
        self.assertIn("Sha256::digest(candidate.as_bytes())", control)
        self.assertIn(".env_remove(ADMIN_TOKEN_ENV)", (ROOT / "src/cluster.rs").read_text(encoding="utf-8"))
        self.assertIn(".env_remove(ADMIN_TOKEN_FILE_ENV)", (ROOT / "src/cluster.rs").read_text(encoding="utf-8"))
        self.assertIn("PAM_ADMIN_TOKEN_FILE", (ROOT / "src/admin_auth.rs").read_text(encoding="utf-8"))
        self.assertIn("Starting = 1", worker_state)
        self.assertIn('if json { "diagnostics" } else { "metrics" }', commands)
        self.assertIn("parse_control_plane_diagnostics(&body)", commands)
        self.assertIn('option == "--json"', main)
        self.assertIn("pam-top.ndjson", docs)


if __name__ == "__main__":
    unittest.main()
