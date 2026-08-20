import json
import struct
import subprocess
import tempfile
import unittest
import zlib
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "scripts" / "product-visual-evidence.py"


def chunk(kind: bytes, payload: bytes) -> bytes:
    return struct.pack(">I", len(payload)) + kind + payload + struct.pack(">I", zlib.crc32(kind + payload))


def png(path: Path, colors: list[str], width: int = 60, height: int = 10) -> None:
    pixels = bytearray()
    segment = width // len(colors)
    for _ in range(height):
        pixels.append(0)
        for x in range(width):
            color = colors[min(x // segment, len(colors) - 1)]
            pixels.extend(int(color[index : index + 2], 16) for index in (1, 3, 5))
    header = struct.pack(">IIBBBBB", width, height, 8, 2, 0, 0, 0)
    path.write_bytes(
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", header)
        + chunk(b"IDAT", zlib.compress(bytes(pixels)))
        + chunk(b"IEND", b"")
    )


class ProductVisualEvidenceTest(unittest.TestCase):
    def setUp(self) -> None:
        self.temporary = tempfile.TemporaryDirectory(prefix="pam-visual-evidence-")
        self.directory = Path(self.temporary.name)
        self.tokens = self.directory / "tokens.json"
        self.colors = {
            "background": "#0b1120",
            "surface": "#111827",
            "surfaceRaised": "#182235",
            "foreground": "#f8fafc",
            "mutedForeground": "#cbd5e1",
            "border": "#475569",
            "primary": "#4ade80",
            "onPrimary": "#052e16",
            "success": "#4ade80",
            "warning": "#fbbf24",
            "danger": "#fb7185",
            "focus": "#68ded2",
        }
        self.tokens.write_text(
            json.dumps(
                {
                    "schemaVersion": 1,
                    "themes": [
                        {"modeCode": 1, "name": "light", "colors": self.colors},
                        {"modeCode": 2, "name": "dark", "colors": self.colors},
                    ],
                }
            ),
            encoding="utf-8",
        )

    def tearDown(self) -> None:
        self.temporary.cleanup()

    def run_verifier(self, native: Path, desktop: Path, output: Path) -> subprocess.CompletedProcess[str]:
        return subprocess.run(
            [
                "python3",
                str(SCRIPT),
                "--tokens",
                str(self.tokens),
                "--native",
                str(native),
                "--desktop",
                str(desktop),
                "--mode-code",
                "2",
                "--output",
                str(output),
            ],
            text=True,
            capture_output=True,
            check=False,
        )

    def test_accepts_bounded_semantic_captures(self) -> None:
        anchors = [self.colors[role] for role in ("background", "surface", "foreground", "primary", "focus", "danger")]
        native = self.directory / "native.png"
        desktop = self.directory / "desktop.png"
        output = self.directory / "evidence.json"
        png(native, anchors)
        png(desktop, anchors)

        result = self.run_verifier(native, desktop, output)

        self.assertEqual(result.returncode, 0, result.stderr)
        report = json.loads(output.read_text(encoding="utf-8"))
        self.assertTrue(report["passed"])
        self.assertEqual([capture["surfaceCode"] for capture in report["captures"]], [2, 3])
        self.assertEqual(report["modeCode"], 2)
        self.assertEqual(report["toleranceChannelDelta"], 12)
        self.assertTrue(all(anchor["passed"] for capture in report["captures"] for anchor in capture["anchors"]))

    def test_rejects_missing_anchor_and_existing_output(self) -> None:
        anchors = [self.colors[role] for role in ("background", "surface", "foreground", "primary", "focus")]
        native = self.directory / "native.png"
        desktop = self.directory / "desktop.png"
        png(native, anchors)
        png(desktop, anchors)
        output = self.directory / "evidence.json"

        missing = self.run_verifier(native, desktop, output)
        self.assertEqual(missing.returncode, 1)
        self.assertFalse(json.loads(output.read_text(encoding="utf-8"))["passed"])

        existing = self.run_verifier(native, desktop, output)
        self.assertEqual(existing.returncode, 1)
        self.assertIn("refusing to overwrite", existing.stderr)

    def test_rejects_corrupt_png(self) -> None:
        anchors = [self.colors[role] for role in ("background", "surface", "foreground", "primary", "focus", "danger")]
        native = self.directory / "native.png"
        desktop = self.directory / "desktop.png"
        png(native, anchors)
        png(desktop, anchors)
        damaged = bytearray(native.read_bytes())
        damaged[-8] ^= 1
        native.write_bytes(damaged)

        result = self.run_verifier(native, desktop, self.directory / "evidence.json")

        self.assertEqual(result.returncode, 1)
        self.assertIn("invalid PNG checksum", result.stderr)

    def test_rejects_oversized_dimensions_and_symlinks(self) -> None:
        native = self.directory / "native.png"
        desktop = self.directory / "desktop.png"
        oversized_header = struct.pack(">IIBBBBB", 4_001, 1_000, 8, 2, 0, 0, 0)
        native.write_bytes(
            b"\x89PNG\r\n\x1a\n"
            + chunk(b"IHDR", oversized_header)
            + chunk(b"IDAT", zlib.compress(b"\x00"))
            + chunk(b"IEND", b"")
        )
        anchors = [self.colors[role] for role in ("background", "surface", "foreground", "primary", "focus", "danger")]
        png(desktop, anchors)

        oversized = self.run_verifier(native, desktop, self.directory / "oversized.json")
        self.assertEqual(oversized.returncode, 1)
        self.assertIn("bounded pixel budget", oversized.stderr)

        native.unlink()
        native.symlink_to(desktop)
        linked = self.run_verifier(native, desktop, self.directory / "linked.json")
        self.assertEqual(linked.returncode, 1)
        self.assertIn("non-symbolic-link", linked.stderr)


if __name__ == "__main__":
    unittest.main()
