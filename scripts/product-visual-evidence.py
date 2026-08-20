#!/usr/bin/env python3

"""Verify semantic visual parity in bounded Native and Desktop PNG captures."""

from __future__ import annotations

import argparse
import hashlib
import json
import struct
import sys
import zlib
from pathlib import Path

PNG_SIGNATURE = b"\x89PNG\r\n\x1a\n"
MAX_FILE_BYTES = 16 * 1024 * 1024
MAX_PIXELS = 4_000_000
MAX_DECOMPRESSED_BYTES = MAX_PIXELS * 4 + 4096
ANCHORS = ("background", "surface", "foreground", "primary", "focus", "danger")
THRESHOLDS = {
    "background": 0.01,
    "surface": 0.001,
    "foreground": 0.00001,
    "primary": 0.00001,
    "focus": 0.000001,
    "danger": 0.000001,
}


class EvidenceError(ValueError):
    pass


def regular_file(path: Path, label: str) -> bytes:
    if path.is_symlink() or not path.is_file():
        raise EvidenceError(f"{label} must be a regular non-symbolic-link file")
    size = path.stat().st_size
    if size <= 0 or size > MAX_FILE_BYTES:
        raise EvidenceError(f"{label} must contain 1 to {MAX_FILE_BYTES} bytes")
    return path.read_bytes()


def paeth(left: int, above: int, upper_left: int) -> int:
    estimate = left + above - upper_left
    left_distance = abs(estimate - left)
    above_distance = abs(estimate - above)
    diagonal_distance = abs(estimate - upper_left)
    if left_distance <= above_distance and left_distance <= diagonal_distance:
        return left
    return above if above_distance <= diagonal_distance else upper_left


def decode_png(path: Path) -> tuple[int, int, list[tuple[int, int, int]]]:
    data = regular_file(path, "screenshot")
    if not data.startswith(PNG_SIGNATURE):
        raise EvidenceError(f"{path} is not a PNG")
    offset = len(PNG_SIGNATURE)
    width = height = channels = None
    compressed = bytearray()
    saw_end = False
    while offset < len(data):
        if offset + 12 > len(data):
            raise EvidenceError(f"{path} has a truncated PNG chunk")
        length = struct.unpack(">I", data[offset : offset + 4])[0]
        kind = data[offset + 4 : offset + 8]
        end = offset + 12 + length
        if end > len(data):
            raise EvidenceError(f"{path} has a truncated PNG payload")
        payload = data[offset + 8 : offset + 8 + length]
        expected_crc = struct.unpack(">I", data[offset + 8 + length : end])[0]
        if zlib.crc32(kind + payload) & 0xFFFFFFFF != expected_crc:
            raise EvidenceError(f"{path} has an invalid PNG checksum")
        if kind == b"IHDR":
            if width is not None or length != 13:
                raise EvidenceError(f"{path} has an invalid IHDR")
            width, height, depth, color_type, compression, filtering, interlace = struct.unpack(
                ">IIBBBBB", payload
            )
            if depth != 8 or color_type not in (2, 6) or compression or filtering or interlace:
                raise EvidenceError(f"{path} must be an 8-bit non-interlaced RGB/RGBA PNG")
            channels = 3 if color_type == 2 else 4
            if width <= 0 or height <= 0 or width * height > MAX_PIXELS:
                raise EvidenceError(f"{path} dimensions exceed the bounded pixel budget")
        elif kind == b"IDAT":
            compressed.extend(payload)
            if len(compressed) > MAX_FILE_BYTES:
                raise EvidenceError(f"{path} has too much compressed image data")
        elif kind == b"IEND":
            saw_end = True
            if end != len(data):
                raise EvidenceError(f"{path} contains bytes after IEND")
        offset = end
    if width is None or height is None or channels is None or not compressed or not saw_end:
        raise EvidenceError(f"{path} is missing required PNG chunks")
    decompressor = zlib.decompressobj()
    raw = decompressor.decompress(bytes(compressed), MAX_DECOMPRESSED_BYTES + 1)
    if len(raw) > MAX_DECOMPRESSED_BYTES or decompressor.unconsumed_tail:
        raise EvidenceError(f"{path} exceeds the decompressed pixel budget")
    raw += decompressor.flush(MAX_DECOMPRESSED_BYTES + 1 - len(raw))
    expected = height * (1 + width * channels)
    if len(raw) != expected or decompressor.unused_data or not decompressor.eof:
        raise EvidenceError(f"{path} has invalid or oversized pixel data")
    stride = width * channels
    previous = bytearray(stride)
    pixels: list[tuple[int, int, int]] = []
    cursor = 0
    for _ in range(height):
        filter_kind = raw[cursor]
        cursor += 1
        encoded = raw[cursor : cursor + stride]
        cursor += stride
        row = bytearray(stride)
        for index, value in enumerate(encoded):
            left = row[index - channels] if index >= channels else 0
            above = previous[index]
            upper_left = previous[index - channels] if index >= channels else 0
            if filter_kind == 0:
                decoded = value
            elif filter_kind == 1:
                decoded = value + left
            elif filter_kind == 2:
                decoded = value + above
            elif filter_kind == 3:
                decoded = value + ((left + above) // 2)
            elif filter_kind == 4:
                decoded = value + paeth(left, above, upper_left)
            else:
                raise EvidenceError(f"{path} uses an unknown PNG row filter")
            row[index] = decoded & 0xFF
        for index in range(0, stride, channels):
            if channels == 4 and row[index + 3] == 0:
                continue
            pixels.append((row[index], row[index + 1], row[index + 2]))
        previous = row
    return width, height, pixels


def parse_color(value: object) -> tuple[int, int, int]:
    if not isinstance(value, str) or len(value) != 7 or value[0] != "#":
        raise EvidenceError("token colors must use canonical hex")
    try:
        return tuple(int(value[index : index + 2], 16) for index in (1, 3, 5))  # type: ignore[return-value]
    except ValueError as error:
        raise EvidenceError("token colors must use canonical hex") from error


def analyze(pixels: list[tuple[int, int, int]], colors: dict[str, object]) -> dict[str, object]:
    total = len(pixels)
    if total == 0:
        raise EvidenceError("screenshot contains no visible pixels")
    anchors = []
    passed = True
    for role in ANCHORS:
        target = parse_color(colors.get(role))
        distances = [
            max(abs(pixel[channel] - target[channel]) for channel in range(3)) for pixel in pixels
        ]
        matching = sum(distance <= 12 for distance in distances)
        required = max(1, int(total * THRESHOLDS[role]))
        role_passed = matching >= required
        passed = passed and role_passed
        anchors.append(
            {
                "role": role,
                "target": colors[role],
                "closestChannelDelta": min(distances),
                "matchingPixels": matching,
                "requiredPixels": required,
                "passed": role_passed,
            }
        )
    return {"visiblePixels": total, "anchors": anchors, "passed": passed}


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--tokens", required=True, type=Path)
    parser.add_argument("--native", required=True, type=Path)
    parser.add_argument("--desktop", required=True, type=Path)
    parser.add_argument("--mode-code", required=True, type=int, choices=(1, 2))
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    token_bytes = regular_file(args.tokens, "token contract")
    document = json.loads(token_bytes)
    if not isinstance(document, dict) or document.get("schemaVersion") != 1:
        raise EvidenceError("token contract schema is incompatible")
    themes = document.get("themes")
    if not isinstance(themes, list) or len(themes) != 2:
        raise EvidenceError("token contract must contain two themes")
    theme = themes[args.mode_code - 1]
    expected_name = "light" if args.mode_code == 1 else "dark"
    if not isinstance(theme, dict) or theme.get("modeCode") != args.mode_code or theme.get("name") != expected_name:
        raise EvidenceError("token theme order or code is incompatible")
    colors = theme.get("colors")
    if not isinstance(colors, dict):
        raise EvidenceError("token theme colors are missing")
    captures = []
    passed = True
    for surface_code, name, path in ((2, "native", args.native), (3, "desktop", args.desktop)):
        width, height, pixels = decode_png(path)
        result = analyze(pixels, colors)
        passed = passed and bool(result["passed"])
        captures.append(
            {
                "surfaceCode": surface_code,
                "name": name,
                "width": width,
                "height": height,
                "bytes": path.stat().st_size,
                "sha256": hashlib.sha256(path.read_bytes()).hexdigest(),
                **result,
            }
        )
    report = {
        "schemaVersion": 1,
        "modeCode": args.mode_code,
        "tokenSha256": hashlib.sha256(token_bytes).hexdigest(),
        "toleranceChannelDelta": 12,
        "captures": captures,
        "passed": passed,
    }
    if args.output.exists():
        raise EvidenceError("refusing to overwrite existing visual evidence")
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, indent=2, separators=(",", ": ")) + "\n", encoding="utf-8")
    print(json.dumps(report, indent=2))
    return 0 if passed else 1


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (EvidenceError, json.JSONDecodeError, OSError, zlib.error) as error:
        print(f"visual evidence rejected: {error}", file=sys.stderr)
        raise SystemExit(1)
