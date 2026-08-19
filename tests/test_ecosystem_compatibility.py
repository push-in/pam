import json
import subprocess
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parent.parent
SCRIPT = ROOT / "scripts" / "ecosystem-compatibility.py"


class EcosystemCompatibilityTests(unittest.TestCase):
    def run_script(self, *arguments: str, stdin: str = "") -> subprocess.CompletedProcess[str]:
        return subprocess.run(
            [str(SCRIPT), *arguments],
            cwd=ROOT,
            input=stdin,
            text=True,
            capture_output=True,
            check=False,
        )

    def test_matrix_is_unique_and_uses_sequential_role_codes(self) -> None:
        result = self.run_script("matrix")
        self.assertEqual(result.returncode, 0, result.stderr)
        packages = json.loads(result.stdout)
        repositories = [package["repository"] for package in packages]
        self.assertEqual(len(repositories), 27)
        self.assertEqual(len(repositories), len(set(repositories)))
        self.assertEqual({package["roleCode"] for package in packages}, {1, 2, 3, 4})

    def test_inventory_fails_closed_for_an_unregistered_repository(self) -> None:
        matrix = json.loads(self.run_script("matrix").stdout)
        inventory = "\n".join(package["repository"] for package in matrix)
        result = self.run_script("inventory", stdin=f"{inventory}\npam-native-forgotten\n")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("pam-native-forgotten", result.stderr)

    def test_inventory_includes_the_official_mobile_ui_package(self) -> None:
        matrix = json.loads(self.run_script("matrix").stdout)
        mobile_ui = next(
            package for package in matrix if package["repository"] == "pam-mobile-ui"
        )
        self.assertEqual(mobile_ui["composerName"], "pushinbr/pam-mobile-ui")
        self.assertEqual(mobile_ui["roleCode"], 3)
        self.assertTrue(mobile_ui["requiresNative"])
        self.assertTrue(mobile_ui["testRequired"])
        workflow = (ROOT / ".github/workflows/ecosystem-compatibility.yml").read_text(
            encoding="utf-8"
        )
        self.assertIn("pam-mobile-ui$", workflow)

    def test_checkout_contract_requires_test_and_native_constraints(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            manifest = {
                "name": "pushinbr/pam-native-auth",
                "require": {"php": "^8.4"},
                "scripts": {},
            }
            Path(directory, "composer.json").write_text(json.dumps(manifest), encoding="utf-8")
            result = self.run_script("verify", directory, "pam-native-auth")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("must constrain pushinbr/pam-native", result.stderr)

    def test_reusable_workflow_certifies_the_calling_package_tag(self) -> None:
        workflow = (ROOT / ".github/workflows/ecosystem-compatibility.yml").read_text(
            encoding="utf-8"
        )
        self.assertIn("workflow_call:", workflow)
        self.assertIn("repository: push-in/pam", workflow)
        self.assertIn("github.event.repository.name == matrix.package.repository", workflow)
        self.assertIn("github.ref_type == 'tag'", workflow)

    def test_every_main_publication_runs_the_ecosystem_matrix(self) -> None:
        workflow = (ROOT / ".github/workflows/ecosystem-compatibility.yml").read_text(
            encoding="utf-8"
        )
        push = workflow.split("  push:\n", 1)[1].split("  pull_request:\n", 1)[0]
        self.assertIn("branches: [main]", push)
        self.assertNotIn("paths:", push)

    def test_packages_run_on_the_latest_compatible_dependency_graph(self) -> None:
        workflow = (ROOT / ".github/workflows/ecosystem-compatibility.yml").read_text(
            encoding="utf-8"
        )
        self.assertIn("--check-lock", workflow)
        self.assertNotIn("--no-check-lock", workflow)
        preflight = "composer update --working-dir=package --dry-run"
        installation = "composer update --working-dir=package --no-interaction"
        self.assertIn(preflight, workflow)
        self.assertIn(installation, workflow)
        self.assertLess(workflow.index(preflight), workflow.index(installation))
        self.assertLess(workflow.index(installation), workflow.index("composer test"))

    def test_every_package_runs_on_both_supported_php_series(self) -> None:
        workflow = (ROOT / ".github/workflows/ecosystem-compatibility.yml").read_text(
            encoding="utf-8"
        )
        self.assertIn("php: ['8.4', '8.5']", workflow)
        self.assertIn("php-version: ${{ matrix.php }}", workflow)
        self.assertIn("/ PHP ${{ matrix.php }}", workflow)
        self.assertNotIn("php-version: '8.4'", workflow)

    def test_every_package_tests_latest_and_lowest_constraint_graphs(self) -> None:
        workflow = (ROOT / ".github/workflows/ecosystem-compatibility.yml").read_text(
            encoding="utf-8"
        )
        self.assertEqual(workflow.count("Preflight the lowest compatible dependency graph"), 1)
        self.assertEqual(workflow.count("--prefer-lowest"), 2)
        self.assertEqual(workflow.count("composer test --working-dir=package"), 2)
        lowest_preflight = workflow.index("Preflight the lowest compatible dependency graph")
        lowest_install = workflow.index("Install the lowest compatible dependency graph")
        lowest_tests = workflow.index("Run package tests on the lowest graph")
        self.assertLess(lowest_preflight, lowest_install)
        self.assertLess(lowest_install, lowest_tests)
        self.assertIn("Verify the PAM Native candidate in the lowest graph", workflow)
        self.assertIn("timeout-minutes: 10", workflow)
        self.assertIn("timeout-minutes: 30", workflow)

    def test_dependency_resolution_is_non_executable_and_both_graphs_are_audited(self) -> None:
        workflow = (ROOT / ".github/workflows/ecosystem-compatibility.yml").read_text(
            encoding="utf-8"
        )
        update_commands = [
            line.strip()
            for line in workflow.splitlines()
            if line.strip().startswith("composer update") or line.strip().startswith("--prefer")
        ]
        self.assertEqual(workflow.count("--no-plugins --no-scripts"), 4)
        self.assertTrue(update_commands)
        self.assertEqual(workflow.count("composer audit --working-dir=package --locked"), 2)
        self.assertEqual(workflow.count("--abandoned=fail"), 2)
        self.assertEqual(workflow.count("Dependency lock is empty; there are no packages to audit."), 2)
        self.assertLess(
            workflow.index("Audit the latest compatible dependency graph"),
            workflow.index("Run package tests\n"),
        )
        self.assertLess(
            workflow.index("Audit the lowest compatible dependency graph"),
            workflow.index("Run package tests on the lowest graph"),
        )

    def test_evidence_aggregates_every_package_php_and_graph_combination(self) -> None:
        matrix = json.loads(self.run_script("matrix").stdout)
        with tempfile.TemporaryDirectory() as directory:
            evidence = Path(directory, "results")
            evidence.mkdir()
            commit = "a" * 40
            for package in matrix:
                for php in ("8.4", "8.5"):
                    result = {
                        "schemaVersion": 1,
                        "resultCode": 1,
                        "repository": package["repository"],
                        "composerName": package["composerName"],
                        "roleCode": package["roleCode"],
                        "phpSeries": php,
                        "graphCodes": [1, 2],
                        "pamCommit": commit,
                        "packageCommit": "b" * 40,
                        "nativeCandidateCommit": "e" * 40 if package["requiresNative"] else None,
                        "graphs": [
                            {"graphCode": 1, "lockSha256": "c" * 64},
                            {"graphCode": 2, "lockSha256": "d" * 64},
                        ],
                    }
                    name = f"{package['repository']}-php{php.replace('.', '')}.json"
                    Path(evidence, name).write_text(json.dumps(result), encoding="utf-8")
            output = Path(directory, "ecosystem.json")
            aggregated = self.run_script("evidence", str(evidence), str(output))
            self.assertEqual(aggregated.returncode, 0, aggregated.stderr)
            report = json.loads(output.read_text(encoding="utf-8"))
            self.assertEqual(report["resultCode"], 1)
            self.assertEqual(report["packageCount"], 27)
            self.assertEqual(report["combinationCount"], 54)
            self.assertEqual(report["graphExecutionCount"], 108)
            self.assertEqual(report["pamCommit"], commit)
            self.assertEqual(report["nativeCandidateCommit"], "e" * 40)

    def test_evidence_fails_closed_when_a_combination_is_missing(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory, "ecosystem.json")
            aggregated = self.run_script("evidence", directory, str(output))
            self.assertNotEqual(aggregated.returncode, 0)
            self.assertIn("missing ecosystem evidence combinations", aggregated.stderr)

    def test_workflow_publishes_validated_aggregate_evidence(self) -> None:
        workflow = (ROOT / ".github/workflows/ecosystem-compatibility.yml").read_text(
            encoding="utf-8"
        )
        self.assertIn("name: ecosystem-result-${{ matrix.package.repository }}-php${{ matrix.php }}", workflow)
        self.assertIn("pattern: ecosystem-result-*", workflow)
        self.assertIn("merge-multiple: true", workflow)
        self.assertIn("ecosystem-compatibility.py evidence", workflow)
        self.assertIn("name: ecosystem-compatibility-evidence", workflow)
        self.assertEqual(workflow.count("retention-days: 30"), 2)
        self.assertIn("package_commit=$(git -C package rev-parse HEAD)", workflow)
        self.assertIn("native_candidate_commit=$(git -C pam-native-candidate rev-parse HEAD)", workflow)
        self.assertIn("LATEST_LOCK_SHA256", workflow)
        self.assertIn("LOWEST_LOCK_SHA256", workflow)

    def test_native_core_tags_bind_every_dependent_to_the_candidate(self) -> None:
        workflow = (ROOT / ".github/workflows/ecosystem-compatibility.yml").read_text(
            encoding="utf-8"
        )
        self.assertIn("repository: push-in/pam-native-php", workflow)
        self.assertIn("ref: ${{ github.ref }}", workflow)
        self.assertIn("matrix.package.requiresNative", workflow)
        self.assertIn("repositories.pam-native-candidate", workflow)
        self.assertIn('versions: {"pushinbr/pam-native": $version}', workflow)
        self.assertIn('.name == "pushinbr/pam-native" and', workflow)
        self.assertIn(".version == $version and", workflow)
        self.assertIn('.dist.type == "path" and', workflow)
        self.assertIn('.dist.url == "../pam-native-candidate"', workflow)
        self.assertLess(
            workflow.index("repositories.pam-native-candidate"),
            workflow.index("Preflight the latest compatible dependency graph"),
        )

    def test_manual_runtime_release_certifies_the_exact_requested_tag(self) -> None:
        workflow = (ROOT / ".github/workflows/ecosystem-compatibility.yml").read_text(
            encoding="utf-8"
        )
        release = (ROOT / ".github/workflows/release.yml").read_text(encoding="utf-8")
        self.assertIn("pam_ref:", workflow)
        self.assertEqual(workflow.count("inputs.pam_ref != '' && inputs.pam_ref"), 4)
        self.assertIn(
            "group: ecosystem-compatibility-${{ inputs.pam_ref != '' && inputs.pam_ref || github.ref }}",
            workflow,
        )
        self.assertIn(
            "pam_ref: ${{ github.event_name == 'workflow_dispatch' && inputs.release_tag || github.ref }}",
            release,
        )
        gate = release.split("  ecosystem-compatibility:\n", 1)[1].split("\n  android-runtime:", 1)[0]
        self.assertIn("uses: ./.github/workflows/ecosystem-compatibility.yml", gate)
        self.assertIn("inputs.release_tag", gate)

    def test_checkout_contract_requires_a_publication_gate(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            manifest = {
                "name": "pushinbr/pam-native-auth",
                "require": {"php": "^8.4", "pushinbr/pam-native": "^0.6.1"},
                "scripts": {"test": "php tests/run.php"},
            }
            Path(directory, "composer.json").write_text(json.dumps(manifest), encoding="utf-8")
            result = self.run_script("verify", directory, "pam-native-auth")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("must certify every publication tag", result.stderr)


if __name__ == "__main__":
    unittest.main()
