import pathlib
import unittest


ROOT = pathlib.Path(__file__).resolve().parents[1]


class CommunityExperienceGateTest(unittest.TestCase):
    def test_gate_creates_and_develops_every_official_project_from_temp(self) -> None:
        script = (ROOT / "scripts/community-experience-gate.sh").read_text(encoding="utf-8")
        self.assertIn("mktemp -d -t pam-community-gate", script)
        self.assertIn('init "${directory}" --template "${template}" --no-interaction', script)
        self.assertIn('"${pam_bin}" dev .', script)
        self.assertIn("init_server raw", script)
        self.assertIn("init_server http", script)
        self.assertIn("init_server laravel", script)
        self.assertIn("init_mobile mobile", script)
        self.assertIn("init_mobile native-ui", script)
        self.assertIn("screencap -p", script)
        self.assertIn("PluginException", script)

    def test_release_publication_requires_a_real_first_run(self) -> None:
        workflow = (ROOT / ".github/workflows/release.yml").read_text(encoding="utf-8")
        self.assertIn("community-first-run:", workflow)
        self.assertIn("scripts/community-experience-gate.sh mobile", workflow)
        publish = workflow.split("\n  publish:\n", maxsplit=1)[1]
        self.assertIn("community-first-run", publish.split("\n    runs-on:", maxsplit=1)[0])


if __name__ == "__main__":
    unittest.main()
