# PAM documentation

PAM is one ecosystem operated through one command: `pam`. Start with the user
journey; open architecture and protocol references only when you need them.

## Start here

1. [Install PAM and create a project](getting-started.md)
2. Run `pam` for the guided launcher
3. Run `pam doctor --fix` inside the project
4. Run `pam dev`
5. Use the generated [CLI reference](cli-reference.md)

The practical [CLI workflow guide](cli.md) explains contextual behavior and
machine output. Package authors can use the [CLI extension contract](extending-cli.md).
Automation can obtain the command catalog with `pam catalog --json` and its
offline validation contract with `pam catalog --schema`.
Release automation can compare two validated snapshots with
`pam catalog --compat baseline.json candidate.json --json` and reject breaking
removals, group moves, or loss of structured JSON support.
The report contract is embedded and available offline with
`pam catalog --compat-schema`.

## Products

| Product | Build | Start |
| --- | --- | --- |
| PAM API | Persistent HTTP and realtime services | `pam init app --template api` |
| PAM Laravel | Laravel hosted by the persistent runtime | [`laravel.md`](laravel.md) |
| PAM Octane | Laravel Octane on the PAM transport | [`../packages/octane/README.md`](../packages/octane/README.md) |
| PAM Native | Android and iOS applications in PHP | [`../pam-native/README.md`](../pam-native/README.md) |
| PAM Desktop | Servo-hosted desktop applications | `pam init app --template desktop` |

PAM Native is certified with real generated hosts on Android and iOS. The
operational contracts and release checklists live in [Android](android.md) and
[iOS](ios.md).

Every product requires the PAM CLI. Official releases manage their private PHP
runtime and verified Composer toolchain; product SDKs and plugins are project
dependencies managed through `pam add`, `pam remove`, and `pam doctor`.

## Guides

- [Composer packages and ecosystem boundaries](packages.md)
- [Signed plugin registry and trust rotation](plugin-registry.md)
- [Plugin registry ceremony and publication operations](registry-operations.md)
- [CLI workflow and automation](cli.md)
- [Application and package commands](extending-cli.md)
- [Laravel lifecycle and production behavior](laravel.md)
- [PAM Octane security boundaries](octane-security.md)
- [Async runtime](async-runtime.md)
- [Production operations](production.md)
- [PAM Process Manager](process-manager.md)
- [Cross-surface observability and OTLP certification](observability.md)
- [Product semantic screenshot evidence](product-visual-evidence.md)
- [Signed clean-host distribution evidence](distribution-evidence.md)
- [Desktop host diagnostic schema](schemas/desktop-host-doctor.schema.json)
- [Control-plane diagnostic schema](schemas/control-plane-diagnostics.schema.json)
- [Control-plane health schema](schemas/control-plane-health.schema.json)
- [`pam top` NDJSON sample schema](schemas/top-sample.schema.json)
- [Delivery publication ledger](publication-ledger.md)
- [Compatibility](compatibility.md)
- [Server transport plugins](server-transports.md)

PAM Native maintains its focused guides under
[`pam-native/docs`](../pam-native/docs/README.md), including components,
navigation, capabilities, performance, plugins, and release validation.

## Concepts and internals

- [Architecture](architecture.md)
- [Native API](native-api.md)
- [Runtime and protocol documentation](../README.md#how-it-works)

## Documentation contract

- Quick starts install PAM before invoking `pam init`.
- Contextual commands are shown first; explicit `pam mobile`/`pam desktop`
  forms are documented as advanced automation interfaces.
- Coded project types, starters, platforms, states, and variants are sequential
  integer enums beginning at `1`.
- CLI reference content is generated from the command catalog with
  `pam docs:generate` and verified in CI with `--check`.
- Copyable examples must be executable or covered by a smoke contract.
