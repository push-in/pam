# Product visual evidence

PAM Product uses semantic screenshot evidence instead of requiring identical
pixels from different native renderers, densities, font engines, and window
sizes. The verifier proves that real Native and Desktop captures visibly apply
the same versioned theme anchors. It does not claim identical layout or complete
WCAG conformance from color presence alone.

Capture a deterministic Product state in the same light or dark mode on both
surfaces. Native already exposes scoped Android and iOS screenshot commands;
the Desktop PNG must come from its platform capture workflow:

```bash
pam mobile screenshot apps/native \
  --output artifacts/screenshots/product-native-dark.png

# Capture the PAM Desktop window with the platform driver into:
# apps/desktop/artifacts/screenshots/product-desktop-dark.png

pam desktop visual verify apps/desktop \
  --name product.dark \
  --actual artifacts/screenshots/product-desktop-dark.png

python3 scripts/product-visual-evidence.py \
  --tokens packages/contracts/design-tokens.json \
  --native apps/native/artifacts/screenshots/product-native-dark.png \
  --desktop apps/desktop/artifacts/screenshots/product-desktop-dark.png \
  --mode-code 2 \
  --output artifacts/product-visual-dark.json
```

Mode codes are sequential integers: `1` light and `2` dark. Surface codes in
the report are `2` Native and `3` Desktop.

The verifier is dependency-free and fail-closed. It:

- accepts only regular, non-symlink PNG files up to 16 MiB;
- validates PNG chunk CRCs, 8-bit RGB/RGBA encoding, filters, decompression and
  a four-million-pixel ceiling;
- records dimensions, byte size and SHA-256 for both captures plus the exact
  token-contract SHA-256;
- measures background, surface, foreground, primary, focus, and danger anchors
  with a declared per-channel tolerance of 12;
- requires minimum pixel coverage per role and refuses to overwrite evidence.

Run both integer modes before making a cross-theme release claim. A passing
report proves semantic anchor presence in the supplied captures only. It must
travel with those exact PNGs and does not replace keyboard, screen-reader,
Dynamic Type, reduced-motion, forced-color, or manual platform review.

Desktop protocol 6 deliberately leaves capture to an operating-system portal or
test driver. The authenticated host's `visual verify` command then decodes the
project-scoped PNG, rejects traversal and oversized inputs, and compares exact
normalized RGBA pixels with the reviewed golden. The Product semantic verifier
adds portable token-anchor checks across different Native/Desktop renderers.

When the workspace contains the exact light and dark filenames shown by this
guide, `pam package` indexes both reports, all four captures, and the shared
design-token digest under `visualEvidence` in `dist/product-release.json`.
`pam release:verify` rehashes every referenced file, checks ordered integer mode
and surface codes, and proves that each report binds the same capture bytes.
Omit all visual files for an ordinary integrity release; providing only one mode
or an incomplete capture set fails packaging rather than producing a partial
certification.

The verifier and adversarial fixtures run in the main CI job. Clean-host visual
certification additionally requires running both capture commands on the target
platforms and retaining the exact PNG files beside the generated report.

The reusable `Android Product visual evidence` workflow builds only the
required x86_64 PHP 8.5 runtime and native engine, generates a clean Product
workspace, performs a Composer dry-run before installation, audits release
authority, and captures the real API 36 app in forced light and dark modes. Its
emulator action and PAM Native checkout are pinned to immutable commits. A
workflow definition is not execution evidence: Android certification begins
only after a clean hosted run publishes both PNGs, their checksum file, audit,
and bounded log artifact.

The reusable `Desktop Product visual evidence` workflow pins PAM
Desktop 1.2.1 by commit, builds the real Servo host, launches the generated
Product application on an isolated Xvfb display and captures its window by host
PID in both themes. Each PNG is decoded through `pam desktop visual`, hashed,
and uploaded with bounded host/display logs. This is also only implemented
automation until a hosted run publishes its artifacts.

The scheduled/manual `Product cross-surface visual certification` workflow
calls both reusable capture workflows and refuses to certify unless both
succeed. It compares their token documents byte-for-byte, runs the semantic
verifier for integer modes `1` and `2`, records the exact PAM, PAM Native and
PAM Desktop revisions, and publishes one 30-day evidence bundle with a sorted,
self-verified `SHA256SUMS`. The workflow definition is reproducible machinery;
only its successful hosted artifact is certification evidence.
