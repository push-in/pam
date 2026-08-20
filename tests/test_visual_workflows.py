import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]


class VisualWorkflowContractTest(unittest.TestCase):
    def test_ios_capture_is_a_fatal_validated_cli_gate(self) -> None:
        workflow = (ROOT / ".github/workflows/ios-native.yml").read_text(encoding="utf-8")
        capture = (
            'target/debug/pam mobile ios:screenshot "${PAM_IOS_FIXTURE}" '
            "\\\n            --output artifacts/screenshots/ios-simulator.png"
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

        self.assertIn("--template product", workflow)
        self.assertIn("--no-dev --no-interaction --prefer-dist --dry-run", workflow)
        self.assertLess(
            workflow.index("--no-dev --no-interaction --prefer-dist --dry-run"),
            workflow.index("--no-dev --no-interaction --prefer-dist\n"),
        )
        self.assertIn(
            "reactivecircus/android-emulator-runner@a421e43855164a8197daf9d8d40fe71c6996bb0d",
            workflow,
        )
        self.assertIn("api-level: 36", workflow)
        self.assertIn("pam mobile run", workflow)
        self.assertIn("product-native-light.png", workflow)
        self.assertIn("product-native-dark.png", workflow)
        self.assertEqual(workflow.count("pam mobile screenshot"), 2)
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
        self.assertEqual(workflow.count("actions/download-artifact@v4"), 2)
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
