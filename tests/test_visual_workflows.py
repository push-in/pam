import re
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]


class VisualWorkflowContractTest(unittest.TestCase):
    def test_ios_capture_is_a_fatal_validated_cli_gate(self) -> None:
        workflow = (ROOT / ".github/workflows/ios-native.yml").read_text(encoding="utf-8")
        capture = (
            '"${GITHUB_WORKSPACE}/target/debug/pam" ios:screenshot . '
            "\\\n              --output artifacts/screenshots/ios-simulator.png"
        )

        self.assertIn(capture, workflow)
        self.assertNotIn('simctl io "${simulator}" screenshot', workflow)
        self.assertIn(
            'test -s "${PAM_IOS_FIXTURE}/artifacts/screenshots/ios-simulator.png"',
            workflow,
        )
        self.assertIn("ios-simulator.png.sha256", workflow)
        self.assertIn(
            "${{ runner.temp }}/pam-ios-fixture/artifacts/screenshots/ios-simulator.png",
            workflow,
        )

    def test_android_product_capture_is_real_pinned_and_bimodal(self) -> None:
        workflow = (ROOT / ".github/workflows/android-visual.yml").read_text(
            encoding="utf-8"
        )
        script = (ROOT / ".github/scripts/product-visual-android.sh").read_text(
            encoding="utf-8"
        )

        self.assertIn("--template product", workflow)
        dry_run = "pam composer install --no-interaction --no-progress --prefer-dist --dry-run"
        self.assertIn(dry_run, workflow)
        self.assertLess(
            workflow.index(dry_run),
            workflow.index("pam composer install --no-interaction --no-progress --prefer-dist &&"),
        )
        # The screen renders on the host before it reaches a device.
        self.assertIn("pam composer test", workflow)
        self.assertIn(
            "reactivecircus/android-emulator-runner@a421e43855164a8197daf9d8d40fe71c6996bb0d",
            workflow,
        )
        self.assertIn("api-level: 36", workflow)
        self.assertIn("profile: pixel_6", workflow)
        self.assertIn("bash .github/scripts/product-visual-android.sh", workflow)
        self.assertIn("PAM_NATIVE_UI_REF: v", workflow)
        self.assertIn("pull_request:", workflow)
        self.assertIn("record_goldens", workflow)
        # Light and dark: a capture, pixel geometry and the golden comparison.
        self.assertIn("for mode in light dark", script)
        self.assertIn("product-native-${mode}.png", script)
        self.assertIn('"${tools}/geometry.py"', script)
        self.assertIn("scripts/product-golden.py", script)
        self.assertIn("tests/golden/product/android", script)
        for mode in ("light", "dark"):
            self.assertTrue((ROOT / f"tests/golden/product/android/product-native-{mode}.png").is_file())
        self.assertIn("product-native-light.png", workflow)
        self.assertIn("product-native-dark.png", workflow)
        self.assertIn("if-no-files-found: error", workflow)
        self.assertIn("retention-days: 30", workflow)
        self.assertIn("workflow_call:", workflow)
        self.assertIn("packages/contracts/design-tokens.json", workflow)

    def test_desktop_product_capture_uses_the_real_pinned_servo_host(self) -> None:
        workflow = (ROOT / ".github/workflows/desktop-visual.yml").read_text(
            encoding="utf-8"
        )

        self.assertIn("c89ea15a78eb3509a8611e42302c867c3c09bbbf", workflow)
        self.assertIn("cargo build --locked --manifest-path pam-desktop/Cargo.toml", workflow)
        self.assertIn("--no-dev --no-interaction --prefer-dist --dry-run", workflow)
        self.assertIn("Xvfb", workflow)
        self.assertIn("xdotool search --onlyvisible --pid", workflow)
        self.assertIn("import -display", workflow)
        self.assertIn("capture_theme light Light", workflow)
        self.assertIn("capture_theme dark Dark", workflow)
        self.assertEqual(workflow.count("pam desktop visual verify"), 1)
        self.assertIn("product-desktop-light.png", workflow)
        self.assertIn("product-desktop-dark.png", workflow)
        self.assertIn("if-no-files-found: error", workflow)
        self.assertIn("retention-days: 30", workflow)
        self.assertIn("workflow_call:", workflow)
        self.assertIn("packages/contracts/design-tokens.json", workflow)

    def test_cross_surface_workflow_requires_both_surfaces_and_both_modes(self) -> None:
        workflow = (ROOT / ".github/workflows/product-visual.yml").read_text(
            encoding="utf-8"
        )

        self.assertIn("uses: ./.github/workflows/android-visual.yml", workflow)
        self.assertIn("uses: ./.github/workflows/desktop-visual.yml", workflow)
        self.assertIn("needs: [android, desktop]", workflow)
        self.assertEqual(len(re.findall(r"actions/download-artifact@(?:v4\b|[0-9a-f]{40} # v4\.)", workflow)), 2)
        self.assertIn("cmp \\", workflow)
        self.assertEqual(workflow.count("scripts/product-visual-evidence.py"), 2)
        self.assertIn("--mode-code 1", workflow)
        self.assertIn("--mode-code 2", workflow)
        self.assertIn("sha256sum --check --strict SHA256SUMS", workflow)
        self.assertIn("pam-product-visual-certified-${{ github.sha }}", workflow)
        self.assertNotIn("continue-on-error", workflow)
        self.assertIn("if-no-files-found: error", workflow)
        self.assertIn("retention-days: 30", workflow)


if __name__ == "__main__":
    unittest.main()
