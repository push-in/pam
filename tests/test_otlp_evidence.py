import json
import subprocess
import tempfile
import unittest
from pathlib import Path


SCRIPT = Path(__file__).parents[1] / "scripts" / "otlp-evidence.py"


class OtlpEvidenceTests(unittest.TestCase):
    def fixture(self, directory: Path) -> None:
        (directory / "collector.log").write_text("accepted\n", encoding="utf-8")
        (directory / "pam.stderr.log").write_text("ready\n", encoding="utf-8")
        (directory / "metadata.json").write_text(
            json.dumps(
                {
                    "source": {"commit": "abc", "dirty": False},
                    "collector": {"image": "collector@sha256:abc"},
                    "protocol": "http/json",
                }
            ),
            encoding="utf-8",
        )
        (directory / "report.json").write_text(
            json.dumps({"passed": True, "gates": {"collector_accepted": True}}),
            encoding="utf-8",
        )

    def run_script(self, directory: Path, *arguments: str) -> subprocess.CompletedProcess[str]:
        return subprocess.run(
            ["python3", str(SCRIPT), str(directory), "1", *arguments],
            check=False,
            capture_output=True,
            text=True,
        )

    def test_manifest_detects_tampered_evidence(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            self.fixture(directory)
            self.assertEqual(self.run_script(directory).returncode, 0)
            self.assertEqual(self.run_script(directory, "--verify").returncode, 0)
            (directory / "collector.log").write_text("tampered\n", encoding="utf-8")
            result = self.run_script(directory, "--verify")
            self.assertEqual(result.returncode, 1)
            self.assertIn("do not match", result.stderr)

    def test_manifest_rejects_failing_and_oversized_evidence(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            self.fixture(directory)
            (directory / "report.json").write_text(
                json.dumps({"passed": False, "gates": {}}), encoding="utf-8"
            )
            self.assertEqual(self.run_script(directory).returncode, 1)
            self.fixture(directory)
            (directory / "collector.log").write_bytes(b"x" * (2 * 1024 * 1024 + 1))
            self.assertEqual(self.run_script(directory).returncode, 1)

    def test_manifest_rejects_dirty_source(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            self.fixture(directory)
            metadata = json.loads((directory / "metadata.json").read_text(encoding="utf-8"))
            metadata["source"]["dirty"] = True
            (directory / "metadata.json").write_text(json.dumps(metadata), encoding="utf-8")
            result = self.run_script(directory)
            self.assertEqual(result.returncode, 1)
            self.assertIn("dirty worktree", result.stderr)

    def test_manifest_rejects_symlinked_artifacts(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            self.fixture(directory)
            target = directory / "outside.log"
            target.write_text("outside\n", encoding="utf-8")
            (directory / "collector.log").unlink()
            (directory / "collector.log").symlink_to(target)
            result = self.run_script(directory)
            self.assertEqual(result.returncode, 1)
            self.assertIn("regular file", result.stderr)


if __name__ == "__main__":
    unittest.main()
