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
        self.assertIn('if [[ -f "${directory}/composer.json" ]]', script)
        self.assertIn('composer.lock is missing in Composer project', script)
        self.assertIn('dependency artifacts unexpectedly exist in Composer-free project', script)
        self.assertNotIn('test -f "${directory}/composer.lock"', script)
        self.assertIn('exec env PAM_PORT="${port}" "${pam_bin}" dev', script)
        self.assertIn('exec "${pam_bin}" dev .', script)
        self.assertIn("stop_dev", script)
        self.assertIn("for attempt in 1 2 3 4 5", script)

    def test_release_publication_requires_a_real_first_run(self) -> None:
        workflow = (ROOT / ".github/workflows/release.yml").read_text(encoding="utf-8")
        self.assertIn("community-first-run:", workflow)
        self.assertIn("scripts/community-experience-gate.sh all", workflow)
        publish = workflow.split("\n  publish:\n", maxsplit=1)[1]
        self.assertIn("community-first-run", publish.split("\n    runs-on:", maxsplit=1)[0])


if __name__ == "__main__":
    unittest.main()
