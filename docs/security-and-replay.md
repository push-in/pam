# Kernel sandbox, flight recorder and trusted bundles

Pam treats these as three separate security boundaries:

1. the package capability sandbox limits what untrusted PHP can ask the kernel to do;
2. the flight recorder captures HTTP evidence to reproduce a failure without retaining secrets;
3. the production manifest, SBOM and Ed25519 signature establish integrity and publisher trust.

## Package capability manifest

Run untrusted PHP in a dedicated process:

```bash
pam sandbox pam.capabilities.json -- plugin.php
```

`pam.capabilities.json` uses stable, sequential integer capability kinds:

| Kind | Capability | Resources |
| ---: | --- | --- |
| `1` | filesystem read | paths relative to the manifest |
| `2` | filesystem write | existing paths relative to the manifest |
| `3` | network | `["*"]` or absent |
| `4` | subprocess | `["*"]` or absent |
| `5` | environment | exact variable names |

```json
{
  "schemaVersion": 1,
  "capabilities": [
    {"kind": 1, "resources": [".", "vendor/package"]},
    {"kind": 2, "resources": ["storage/package"]},
    {"kind": 5, "resources": ["APP_ENV", "PACKAGE_API_KEY"]}
  ]
}
```

On Linux, filesystem policy is enforced with Landlock. Network and process
denials are enforced with seccomp after PAM has started but before PHP boots.
Environment variables not explicitly named are removed. Runtime support paths
needed to load PHP extensions, certificates and timezone data are read-only.

Selective domains and executable allowlists require the PAM broker. Until that
broker is enabled, values other than `["*"]` are rejected. PAM never silently
upgrades `api.example.com` into unrestricted network access. The sandbox fails
closed on platforms without Landlock and seccomp.

This command is the boundary for untrusted package code. Loading an untrusted
Composer package directly into the main Laravel worker does not claim this isolation.

## Flight recorder and replay

Start the application through the bounded recorder:

```bash
pam record index.php --output .pam/incidents/checkout.jsonl
```

Each JSONL entry contains a schema version, integer event kind, sequence,
request ID, method, target, headers, request/response bodies, duration and
SHA-256 digests. Defaults are 64 KiB per body and 64 MiB per recording.

Authorization, cookies, API keys, password/token/secret fields and sensitive
query values become named placeholders before disk I/O. Binary data is base64.
An exclusive file lock prevents worker processes from interleaving partial JSON.

Replay against a live candidate:

```bash
export INCIDENT_AUTH='Bearer ...'
export INCIDENT_TOKEN='...'

pam replay .pam/incidents/checkout.jsonl \
  --url http://127.0.0.1:3000 \
  --secret-env authorization=INCIDENT_AUTH \
  --secret-env token=INCIDENT_TOKEN
```

Secrets are read from the environment and never printed. Replay restores the
requests and fails on the first status or response-body digest divergence. A
truncated request body is not replayed.

The current recorder covers the native HTTP boundary. Database result capture,
clock/random virtualization and native-operation replay remain future contracts;
HTTP replay must not be described as full VM determinism.

## SBOM, integrity and Ed25519 trust

Generate an Ed25519 release key outside the repository:

```bash
openssl genpkey -algorithm ED25519 -out release.key
openssl pkey -in release.key -pubout -out release.pub
```

Build and verify:

```bash
pam build . --output dist --signing-key release.key
pam verify dist --public-key release.pub --require-signature
```

The bundle includes `manifest.json` with sorted SHA-256 file records,
`manifest.sig` with the Ed25519 signature and `sbom.cdx.json` with a
deterministic CycloneDX 1.6 inventory.

The manifest stores algorithm kind `1` (Ed25519) and the SHA-256 key ID derived
from the public DER key. Verification requires an external trusted public key;
an artifact cannot establish its own identity by embedding a replacement key.
Missing, extra, modified, duplicate, symlinked and path-escaping entries fail.
