import json
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]


class DistributionContractTest(unittest.TestCase):
    def test_schema_and_executable_verifier_keep_the_same_integer_contract(self) -> None:
        schema = json.loads(
            (ROOT / "docs/schemas/distribution-evidence.schema.json").read_text(
                encoding="utf-8"
            )
        )
        desktop_schema = json.loads(
            (ROOT / "docs/schemas/desktop-platform-verification.schema.json").read_text(
                encoding="utf-8"
            )
        )
        source = (ROOT / "src/distribution.rs").read_text(encoding="utf-8")

        self.assertEqual(schema["properties"]["schemaVersion"]["const"], 1)
        self.assertEqual(schema["properties"]["surfaceCode"]["enum"], [1, 2, 3])
        self.assertEqual(schema["properties"]["platformCode"]["enum"], [1, 2, 3, 4, 5])
        self.assertEqual(schema["properties"]["architectureCode"]["enum"], [1, 2])
        self.assertEqual(schema["properties"]["packageCode"]["enum"], list(range(1, 11)))
        self.assertEqual(
            schema["$defs"]["check"]["properties"]["checkCode"]["enum"],
            list(range(1, 8)),
        )
        self.assertEqual(
            schema["$defs"]["check"]["properties"]["resultCode"]["const"],
            1,
        )
        self.assertIn("signingPublicKey", schema["required"])
        self.assertIn("manifestSignature", schema["required"])
        self.assertIn("baselineArtifact", schema["required"])
        self.assertIn("provenanceInventory", schema["required"])
        self.assertEqual(schema["properties"]["issuedAtUnix"]["minimum"], 1)
        self.assertEqual(schema["properties"]["expiresAtUnix"]["minimum"], 2)
        self.assertEqual(desktop_schema["properties"]["surfaceCode"]["const"], 3)
        self.assertEqual(
            desktop_schema["properties"]["signatureKindCode"]["enum"], [1, 2, 3]
        )
        self.assertEqual(
            desktop_schema["properties"]["notarizationResultCode"]["enum"], [1, 2]
        )
        for proof in [
            "publisherCertificate",
            "signatureProof",
            "sandboxProof",
            "updateRecoveryProof",
        ]:
            self.assertIn(proof, desktop_schema["required"])
        self.assertIn("deny_unknown_fields", source)
        for enum_name in [
            "SurfaceCode",
            "PlatformCode",
            "ArchitectureCode",
            "PackageCode",
            "CheckCode",
            "ResultCode",
            "DesktopSignatureKindCode",
            "NotarizationResultCode",
        ]:
            self.assertIn(f"enum {enum_name}", source)
        self.assertIn("verify_strict", source)
        self.assertIn("pub fn sign", source)
        self.assertIn("MAX_MANIFEST_BYTES: u64 = 256 * 1024", source)
        self.assertIn("MAX_INVENTORY_BYTES: u64 = 16 * 1024 * 1024", source)
        self.assertIn("UPDATE_MAX_VALIDITY_SECONDS: u64 = 31 * 24 * 60 * 60", source)
        self.assertIn("UPDATE_CLOCK_SKEW_SECONDS: u64 = 5 * 60", source)
        self.assertIn("Component::Normal", source)
        self.assertIn("platformVerificationSha256", source)
        self.assertIn("verify_desktop_platform_evidence", source)

    def test_ci_and_documentation_expose_the_contract(self) -> None:
        workflow = (ROOT / ".github/workflows/ci.yml").read_text(encoding="utf-8")
        catalog = (ROOT / "src/catalog.rs").read_text(encoding="utf-8")
        guide = (ROOT / "docs/distribution-evidence.md").read_text(encoding="utf-8")

        self.assertIn("docs/schemas/distribution-evidence.schema.json", workflow)
        self.assertIn("docs/schemas/desktop-platform-verification.schema.json", workflow)
        self.assertIn("tests.test_distribution_contract", workflow)
        self.assertIn('"distribution:verify"', catalog)
        self.assertIn('"distribution:sign"', catalog)
        self.assertIn("strict Ed25519 verification", guide)
        self.assertIn("rejects", guide)
        self.assertIn("at most 8 GiB", guide)
        self.assertIn("checkCode: 6", guide)
        self.assertIn("platformVerification", guide)
        self.assertIn("can no longer stand in for", guide.replace("\n", " "))


if __name__ == "__main__":
    unittest.main()
