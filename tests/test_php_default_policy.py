import json
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]


class PhpDefaultPolicyTest(unittest.TestCase):
    def test_runtime_catalog_defaults_to_php_85(self) -> None:
        catalog = json.loads((ROOT / "runtime/catalog.json").read_text(encoding="utf-8"))

        self.assertEqual(catalog["default"], "8.5")
        runtime_id = catalog["channels"][catalog["default"]]
        self.assertTrue(runtime_id.startswith("8.5."))
        self.assertTrue(catalog["releases"][runtime_id]["phpVersion"].startswith("8.5."))

    def test_builders_use_the_catalog_default_without_an_override(self) -> None:
        android = (ROOT / "runtime-builder/android/build.sh").read_text(encoding="utf-8")
        ios = (ROOT / "runtime-builder/ios/build.sh").read_text(encoding="utf-8")

        self.assertIn('runtime_selector="${PAM_PHP_VERSION:-8.5}"', android)
        self.assertIn("runtime_selector=${PAM_PHP_VERSION:-8.5}", ios)

    def test_ios_runtime_ships_opcache_with_shared_memory_and_without_jit(self) -> None:
        ios = (ROOT / "runtime-builder/ios/build.sh").read_text(encoding="utf-8")

        self.assertIn("opcache_options=(--disable-opcache-jit)", ios)
        self.assertIn("php_cv_shm_mmap_anon=${opcache_enabled}", ios)
        self.assertIn("php_cv_shm_ipc=no", ios)
        self.assertIn('verify_ios_opcache "${install}" "${name}"', ios)
        self.assertIn("^#define HAVE_SHM_MMAP_ANON 1", ios)
        self.assertIn("^#define HAVE_JIT 1", ios)
        self.assertIn("_zend_file_cache_script_load$", ios)

    def test_generated_projects_require_php_85(self) -> None:
        skeleton = json.loads(
            (ROOT / "packages/skeleton/composer.json").read_text(encoding="utf-8")
        )
        commands = (ROOT / "src/commands.rs").read_text(encoding="utf-8")

        self.assertEqual(skeleton["require"]["php"], "^8.5")
        self.assertNotIn('"php": "^8.4"', commands)
        self.assertGreaterEqual(commands.count('"php": "^8.5"'), 3)

    def test_release_and_container_builds_embed_php_85(self) -> None:
        dockerfile = (ROOT / "Dockerfile").read_text(encoding="utf-8")
        release = (ROOT / ".github/workflows/release.yml").read_text(encoding="utf-8")

        self.assertNotIn("libphp8.4-embed", dockerfile)
        self.assertIn("libphp8.5-embed", dockerfile)
        self.assertIn("phpize8.5", release)
        self.assertIn("./configure --with-php-config=php-config8.5", release)


if __name__ == "__main__":
    unittest.main()
