# Bootstrap source snapshots

PAM source snapshots freeze the exact PHP tree expected at cold start. Before
the Embed SAPI starts, PAM reads and hashes every tracked PHP file, validates the
runtime/ABI contract and rejects additions, removals or mutations. Reading the
tree also warms the operating-system page cache for the immediately following
boot.

This is deliberately a source snapshot, not a serialized Zend VM heap. Native
resources, credentials, database handles, request state and user data are never
captured.

## Create and run

```bash
pam snapshot create . \
  --entry public/index.php \
  --signing-key /run/release/pam-ed25519.pem

pam snapshot verify .pam/bootstrap.snapshot.json \
  --project . \
  --public-key /etc/pam/release.pub \
  --require-signature

pam snapshot run .pam/bootstrap.snapshot.json \
  --project . \
  --public-key /etc/pam/release.pub \
  --require-signature
```

`run` performs the complete verification before application code executes. Add
`--` followed by application arguments when needed.

The deterministic manifest contains the schema version, exact PAM runtime and
native ABI, relative entry point, sorted PHP paths, byte lengths, SHA-256
digests, and optional Ed25519 public-key identity and sidecar signature.

Signed snapshots require an external trusted public key. A key embedded in the
same deployable directory would not establish trust.

## Safety boundaries

Paths must be relative and stay below the selected project root. Symbolic links
are rejected. Individual PHP files are limited to 64 MiB and a snapshot to
100,000 PHP files. `.git`, `.pam`, `node_modules`, `storage` and `target`
directories are excluded because they are mutable or unrelated to runtime PHP.

The runtime refuses a snapshot created by another PAM version or native ABI.
After deploying any PHP change, generate and sign a new snapshot. Never update a
manifest independently from its sources.

PHP upstream does not enable OPcache for the Embed SAPI, so PAM does not pretend
this is a bytecode snapshot. A future bytecode/heap snapshot requires a
PAM-owned PHP build, reproducible serialization and proofs that pointers,
extensions, secrets and native resources cannot cross the snapshot boundary.
Until then, source integrity and deterministic page-cache warming are the safe
production contract.
