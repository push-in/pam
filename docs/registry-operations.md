# Plugin registry operations

This runbook turns the signed-registry format into a repeatable production
operation. It does not claim that the official ceremony has happened. That
claim requires the public evidence listed under “Publication gate”.

## Custody model

Use three independently controlled, offline Ed25519 keys and threshold `2`.
Each custodian generates and retains one encrypted private key on separate
hardware. The coordinator receives only the 32-byte raw public key and detached
64-byte signatures. Never copy private keys, passphrases or seeds into this
repository, CI, issue trackers or chat.

On each offline workstation:

```bash
umask 077
mkdir pam-registry-custodian
cd pam-registry-custodian
openssl genpkey -algorithm ED25519 -aes-256-cbc -out registry-key.pem
openssl pkey -in registry-key.pem -pubout -outform DER \
  | tail -c 32 | xxd -p -c 256 > registry-public.hex
pam registry key-id --public-key "$(cat registry-public.hex)" \
  > registry-key-id.txt
```

The coordinator independently recomputes every key ID. Public keys and key IDs
in `root.json` must be sorted by `keyId`; active, retired and revoked key states
use integer codes `1`, `2` and `3`.

## Canonical payload and detached signing

Prepare a root or catalog JSON with empty `signatures` arrays. Root rotations
also start with an empty `previousSignatures` array. PAM validates bounded
schema, key identities, ordering, compatibility and validity windows before it
writes the exact compact JSON payload used by the verifier:

```bash
pam registry payload --document root.json --output root.payload.json
sha256sum root.json root.payload.json
```

Distribute the payload and both hashes to each custodian through independent
channels. Each custodian compares the payload with the reviewed draft and signs
the exact bytes offline:

```bash
openssl pkeyutl -sign -rawin \
  -inkey registry-key.pem \
  -in root.payload.json \
  -out root.signature.bin
test "$(wc -c < root.signature.bin)" -eq 64
xxd -p -c 256 root.signature.bin > root.signature.hex
```

The coordinator validates that every signature is 128 lowercase hexadecimal
characters, inserts `{ "keyId": "…", "signature": "…" }` records sorted by
`keyId`, and never edits any payload field after signing. For a rotation, the
same next-root payload is signed by the old quorum into `previousSignatures`
and by the new quorum into `signatures`.

## Verification and publication gate

Before initial publication, two witnesses on clean machines must independently:

1. Recreate the canonical payload and compare its SHA-256 with the ceremony
   record.
2. Run `pam registry verify` against the root and first catalog at the recorded
   ceremony time.
3. Resolve at least one Server, Native and Desktop release with their exact
   protocol versions.
4. Confirm every artifact URL is immutable HTTPS and its bytes match the signed
   SHA-256.
5. Confirm no private-key material exists in the release workspace or CI logs.

Example final verification:

```bash
root_sha256=$(sha256sum root.json | cut -d ' ' -f 1)
pam registry verify \
  --root root.json \
  --root-sha256 "$root_sha256" \
  --catalog catalog.json \
  --minimum-sequence 1 \
  --json
```

The official registry is not live until all of these independent artifacts are
public and mutually consistent:

- immutable `root.json` and `catalog.json` URLs;
- root SHA-256 embedded in a tagged PAM release;
- the same fingerprint published through a separately administered channel;
- ceremony minutes naming custodians by role, witness results, timestamps,
  payload hashes and signature key IDs;
- artifact attestations for every advertised release;
- an incident contact and tested emergency-rotation procedure.

Catalog sequence is increased exactly once per publication and never reused.
Generate a new catalog before its seven-day lifetime ends; retain old catalogs
for audit but never serve them as current metadata. Root rotation increments
generation by exactly one and is rehearsed before the current root approaches
expiry.
