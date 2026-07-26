# WASI sandbox and typed RPC

PAM can run a WebAssembly guest without inheriting the host process's
filesystem, network, environment or arguments. The same boundary powers a
versioned RPC protocol whose request and response values are validated against
contracts generated from PHP DTOs.

This is intended for untrusted package logic, portable compute extensions and
language interoperability. It is not a way to grant a guest ambient access to
the host.

## Run a capability-safe WASI module

```bash
pam wasi run extension.wasm \
  --fuel 100000000 \
  --memory-bytes 67108864 \
  --max-output-bytes 8388608 \
  --timeout-ms 30000
```

The default policy is denied:

| Capability | Default | Explicit grant |
| --- | --- | --- |
| Environment | none | `--env NAME` copies only that host variable |
| Filesystem | none | `--read-dir HOST=GUEST` or `--write-dir HOST=GUEST` |
| Network and DNS | none | not available to Preview 1 guests |
| Standard input | empty | `--stdin FILE`, bounded to 64 MiB |
| Guest arguments | module name only | values after `--` |
| CPU | 100 million fuel units | `--fuel N` |
| Linear memory | 64 MiB | `--memory-bytes N`, at most 2 GiB |
| stdout/stderr | 8 MiB each | `--max-output-bytes N`, at most 64 MiB |
| Wall time | 30 seconds | `--timeout-ms N`, at most one hour |

Cranelift compiles the guest with NaN canonicalization. Wasmtime store limits
bound memories, tables and instances. Fuel prevents unbounded computation and
an epoch deadline interrupts a guest that still runs when wall time expires.
The deadline guard is joined after every invocation, so repeated short calls do
not accumulate sleeping host threads.

Filesystem grants resolve the host path before execution. Read mappings receive
read-only directory and file permissions. Write mappings are explicit and
separate. A guest cannot use `..` to escape its preopened guest path.

PAM currently accepts WASI Preview 1 command modules exporting `_start`.
Component Model interfaces and network capabilities are not silently emulated.

## Define the PHP source of truth

Generate portable schemas from attributed DTOs and integer-backed enums first:

```bash
pam contracts bootstrap/contracts.php --output generated/contracts
```

See [Typed contracts](typed-contracts.md) for supported field types and enum
rules. The RPC boundary consumes `contracts.mobile.json` because it is a compact
catalog that retains the PHP class mapping.

Create `pam.rpc.json`:

```json
{
  "schemaVersion": 1,
  "service": "Orders",
  "version": "1.0.0",
  "methods": [
    {
      "kind": 1,
      "name": "createOrder",
      "input": "CreateOrder",
      "output": "OrderCreated",
      "timeoutMs": 2000,
      "idempotent": true
    }
  ]
}
```

`kind` is an integer enum. Version 1 implements unary method kind `1`. Service,
method and contract identifiers are validated; method names must be unique; all
referenced contracts must exist; and deadlines must be finite.

Validate it in CI:

```bash
pam rpc validate pam.rpc.json \
  --contracts generated/contracts/contracts.mobile.json
```

Validation rejects malformed catalogs as well as malformed manifests:

- object and enum kinds outside the supported integer range;
- non-sequential integer enum values;
- duplicate enum cases or object fields;
- missing array item types;
- references to unknown DTOs or enums;
- unknown input/output contracts;
- unsupported method kinds and unbounded deadlines.

## Generate SDKs

```bash
pam rpc generate pam.rpc.json \
  --contracts generated/contracts/contracts.mobile.json \
  --output generated/rpc
```

The output directory must be empty. PAM refuses to overwrite existing generated
files.

| File | Purpose |
| --- | --- |
| `pam-rpc.ts` | TypeScript DTOs, integer enums, transport interface and client |
| `pam_rpc.py` | Python `TypedDict`, `IntEnum`, async transport and client |
| `pam_rpc.rs` | Rust Serde DTOs, integer enums, transport trait and typed client |
| `pam-rpc.manifest.json` | canonical reviewed service manifest |
| `RPC.md` | method, contract, deadline and idempotency reference |

The SDK owns no network policy. Applications inject a transport, so HTTP, Unix
sockets, WebSockets, test doubles and an authenticated broker can use the same
types without placing credentials in generated code.

Rust enums use `#[serde(try_from = "i64", into = "i64")]`; Python uses
`IntEnum`; TypeScript emits numeric enums. A status or discriminator therefore
remains the same sequential integer on every supported wire.

## Invoke a WASI RPC service

Write the raw method input to `request.json`, then run:

```bash
pam rpc wasi pam.rpc.json orders.wasm createOrder request.json \
  --contracts generated/contracts/contracts.mobile.json \
  --fuel 100000000 \
  --memory-bytes 67108864
```

PAM performs these steps:

1. validates the manifest and complete contract catalog;
2. validates the request recursively, including unknown fields, nullability,
   arrays, integer enums, UUID format and numeric bounds;
3. creates a bounded protocol envelope and writes one JSON document to guest
   stdin;
4. executes the module with no filesystem, environment, network or DNS;
5. bounds CPU, memory, output and time using the method deadline;
6. requires a matching response ID and valid integer message kind;
7. rejects unknown envelope fields and recursively validates the result;
8. prints only the validated result JSON.

Use `--request-id ID` when an upstream idempotency key or trace correlation ID
must be preserved. Otherwise PAM generates a process-unique ID.

Request envelope:

```json
{
  "protocolVersion": 1,
  "id": "checkout-01J...",
  "kind": 1,
  "service": "Orders",
  "method": "createOrder",
  "payload": {}
}
```

Success envelope:

```json
{
  "protocolVersion": 1,
  "id": "checkout-01J...",
  "kind": 2,
  "result": {}
}
```

Failure envelope:

```json
{
  "protocolVersion": 1,
  "id": "checkout-01J...",
  "kind": 3,
  "error": {
    "code": 1,
    "message": "rejected"
  }
}
```

Message kinds are sequential integers: request `1`, success `2`, failure `3`.
Error codes are positive integers chosen and documented by the service.

## Language boundary and isolation

The WASI execution path is the hardened in-process guest boundary. Rust, C/C++
and other languages that produce a compatible WASI command module can use it
today.

The generated TypeScript and Python clients are transport-independent host
bindings. Running a normal Node or Python interpreter remains an external
process and must be isolated by the deployment's container, VM or future PAM
sidecar broker. PAM does not describe those interpreters as sandboxed merely
because their messages are typed.

## Production gate

Keep the manifest, generated catalog and SDK output in source control. A drift
gate can regenerate into temporary directories:

```bash
temporary_contracts="$(mktemp -d)"
temporary_rpc="$(mktemp -d)"

pam contracts bootstrap/contracts.php --output "$temporary_contracts"
pam rpc generate pam.rpc.json \
  --contracts "$temporary_contracts/contracts.mobile.json" \
  --output "$temporary_rpc"

diff -ru generated/contracts "$temporary_contracts"
diff -ru generated/rpc "$temporary_rpc"
```

Run untrusted guests with the lowest practical fuel, memory, output and deadline
values. Treat stderr as guest-controlled diagnostic data and apply normal log
redaction before central ingestion.
