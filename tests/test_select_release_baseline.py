import importlib.util
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location(
    "select_release_baseline", ROOT / "scripts/select-release-baseline.py"
)
assert SPEC is not None and SPEC.loader is not None
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


class SelectReleaseBaselineTest(unittest.TestCase):
    def test_selects_latest_older_stable_release_independent_of_api_order(self) -> None:
        releases = [
            {"tagName": "v2.0.0"},
            {"tagName": "nightly"},
            {"tagName": "v1.8.9"},
            {"tagName": "v1.10.0"},
            {"tagName": "v1.9.4"},
        ]
        self.assertEqual(MODULE.select_baseline(releases, "v1.10.1"), "v1.10.0")

    def test_rejects_nonstable_candidate_and_missing_baseline(self) -> None:
        with self.assertRaisesRegex(ValueError, "stable SemVer"):
            MODULE.select_baseline([], "v2.0.0-rc.1")
        with self.assertRaisesRegex(ValueError, "no older stable release"):
            MODULE.select_baseline([{"tagName": "v2.0.0"}], "v1.0.0")

    def test_ignores_malformed_release_entries(self) -> None:
        releases = [None, "v1.0.0", {}, {"tagName": 3}, {"tagName": "v1.0.0"}]
        self.assertEqual(MODULE.select_baseline(releases, "v1.0.1"), "v1.0.0")


if __name__ == "__main__":
    unittest.main()
