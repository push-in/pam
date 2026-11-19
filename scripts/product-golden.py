#!/usr/bin/env python3
"""Record or compare a Product screen golden with the PAM Native UI golden tool.

The status bar and navigation bar change between runs (clock, icons, cutout
height), so they are masked from the app's own layout snapshot insets
(pam-native-ui tools/geometry.py --json); the content is compared from its
own top edge, with the tolerance documented in tools/golden.py.

    product-golden.py --tools pam-native-ui/tools --geometry geometry-light.json \
        --actual product-native-light.png --golden tests/golden/product/android/product-native-light.png \
        [--diff diff.png] [--record]
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--tools", required=True, type=Path)
    parser.add_argument("--geometry", required=True, type=Path)
    parser.add_argument("--actual", required=True, type=Path)
    parser.add_argument("--golden", required=True, type=Path)
    parser.add_argument("--diff", type=Path, default=None)
    parser.add_argument("--record", action="store_true")
    args = parser.parse_args()

    sys.path.insert(0, str(args.tools.resolve()))
    import golden  # noqa: PLC0415 (the tool lives in the pinned pam-native-ui checkout)

    snapshot = json.loads(args.geometry.read_text(encoding="utf-8"))
    width, height = snapshot["screen"]
    _, top, _, bottom = snapshot["insets"]
    masks = [(0, 0, width, top), (0, height - bottom, width, height)]

    if args.record:
        args.golden.parent.mkdir(parents=True, exist_ok=True)
        golden.mask(golden.Image.open(args.actual), masks).save(args.golden)
        print(f"RECORDED {args.golden}")
        return 0
    if not args.golden.is_file():
        print(f"MISSING golden {args.golden}; run the workflow with record_goldens", file=sys.stderr)
        return 1
    ok, detail = golden.compare(args.actual, args.golden, masks, args.diff, insets=(top, bottom))
    print(("MATCH " if ok else "DIFF ") + f"{args.golden.name}: {detail}")
    return 0 if ok else 1


if __name__ == "__main__":
    raise SystemExit(main())
