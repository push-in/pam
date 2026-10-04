import pathlib
import subprocess
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
        self.assertIn("init_mobile native", script)
        self.assertIn("init_mobile native-ui", script)
        self.assertIn("screencap -p", script)
        self.assertIn("PluginException", script)
        self.assertIn('if [[ -f "${directory}/composer.json" ]]', script)
        self.assertIn('composer.lock is missing in Composer project', script)
        self.assertIn('dependency artifacts unexpectedly exist in Composer-free project', script)
        self.assertNotIn('test -f "${directory}/composer.lock"', script)
        self.assertIn('exec setsid env PAM_PORT="${port}" "${pam_bin}" dev', script)
        self.assertIn('exec setsid "${pam_bin}" dev .', script)
        self.assertIn("stop_dev", script)
        self.assertIn("report_error", script)
        self.assertIn("android-logcat.txt", script)
        self.assertIn("screenshot attempt", script)
        self.assertIn("Android launch proven", script)
        self.assertIn("parse-system-anr-wait.py", script)
        self.assertIn("for attempt in 1 2 3 4 5", script)

    def test_release_publication_requires_a_real_first_run(self) -> None:
        workflow = (ROOT / ".github/workflows/release.yml").read_text(encoding="utf-8")
        self.assertIn("community-first-run:", workflow)
        self.assertIn("scripts/community-experience-gate.sh all", workflow)
        publish = workflow.split("\n  publish:\n", maxsplit=1)[1]
        self.assertIn("community-first-run", publish.split("\n    runs-on:", maxsplit=1)[0])

    def test_only_system_launcher_anr_wait_button_is_recovered(self) -> None:
        parser = ROOT / "scripts/parse-system-anr-wait.py"

        def parse(title: str, package: str = "android") -> str:
            hierarchy = (
                'UI hierchary dumped to: /sdcard/window.xml\n'
                '<hierarchy><node package="android" resource-id="android:id/alertTitle" '
                f'text="{title}" />'
                f'<node package="{package}" text="Wait" bounds="[100,200][300,260]" />'
                '</hierarchy>'
            )
            result = subprocess.run(
                ["python3", str(parser)],
                input=hierarchy,
                text=True,
                capture_output=True,
                check=True,
            )
            return result.stdout.strip()

        self.assertEqual(parse("Quickstep isn't responding"), "200 230")
        self.assertEqual(parse("Pixel Launcher isn't responding"), "200 230")
        self.assertEqual(parse("PAM Native isn't responding"), "")
        self.assertEqual(parse("Quickstep isn't responding", "dev.pam.app"), "")


if __name__ == "__main__":
    unittest.main()
