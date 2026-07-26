# Typed contracts

PAM compiles attributed PHP DTOs and integer-backed enums into one portable
contract set for servers, mobile clients, tools and documentation. The PHP
definition remains the source of truth; generated files are deterministic and
must not be edited by hand.

## Define contracts

```php
use Pam\Contract\Data;
use Pam\Contract\Field;

#[Data(description: 'Lifecycle of an order.')]
enum OrderStatus: int
{
    case Pending = 1;
    case Paid = 2;
    case Shipped = 3;
}

#[Data(description: 'Command accepted by the order boundary.')]
final readonly class CreateOrder
{
    /** @param list<string> $tags */
    public function __construct(
        #[Field(format: 'uuid')]
        public string $id,
        public OrderStatus $status,
        #[Field(itemType: 'string')]
        public array $tags,
        #[Field(minimum: 1, maximum: 1000)]
        public int $quantity,
    ) {
    }
}
```

`#[Data]` accepts an optional stable generated name and description. Public,
non-static properties become fields. Supported types are `string`, `int`,
`float`, `bool`, arrays with an explicit `itemType`, other attributed DTOs and
attributed enums. Nullable PHP types become nullable schema/client types.

Enums must be integer-backed and sequential from `1`. This makes persistence,
wire formats, TypeScript and Kotlin bindings agree without string status drift.

## Generate

```bash
pam contracts bootstrap/contracts.php --output generated/contracts
```

The output directory must be empty. PAM refuses to overwrite a populated
directory so a stale or hand-modified contract cannot disappear silently.

The command writes:

| Artifact | Consumer |
| --- | --- |
| `contracts.schema.json` | JSON Schema 2020-12 validation |
| `openapi.components.json` | OpenAPI 3.1 schema components |
| `contracts.mobile.json` | PAM Native and other mobile runtimes |
| `contracts.forms.json` | form/control generation metadata |
| `contracts.mcp.json` | MCP resource descriptors and schemas |
| `contracts.migrations.json` | advisory database column plan |
| `contracts.ts` | TypeScript interfaces and integer enums |
| `Contracts.kt` | Kotlin data classes and integer enums |
| `CONTRACTS.md` | generated human-readable reference |

Migration output is intentionally advisory: review table names, indexes,
relationships and destructive changes before turning it into a database
migration. Enum-like columns are emitted as integers.

## CI drift gate

Generate into a temporary directory and compare it with the committed contract
directory:

```bash
generated_dir="$(mktemp -d)"
pam contracts bootstrap/contracts.php --output "$generated_dir"
diff -ru generated/contracts "$generated_dir"
```

A non-zero diff means the PHP source and distributed artifacts no longer agree.
Run generation intentionally, review all targets, and commit them together.

Contract schema version `1` is validated on both the PHP and Rust boundary.
Duplicate names, unsupported kinds, malformed fields and non-sequential enum
values fail generation instead of producing a partially valid client.
