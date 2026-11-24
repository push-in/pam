# Upgrading Nimbus

## Compatibility policy

The full policy (what is public API, the one-minor deprecation cycle with
`@deprecated` and `Nimbus\Deprecation::trigger()`, and 18 months of security
fixes for 1.x after 2.0) is in [docs/VERSIONING.md](docs/VERSIONING.md).

- Patch releases fix defects without intentionally breaking public contracts.
- Minor releases add compatible public API and may deprecate existing API.
- Major releases may remove APIs only after the documented deprecation window.
- Classes and methods marked `@internal` are excluded from the public BC
  promise but remain covered by tests for the release that contains them.

`composer api:compat` compares the reflection-derived public surface against
`api-surface.json`. Unreviewed additions fail as baseline drift; breaking
symbol, member, signature, inheritance, interface and enum changes fail as
incompatibilities. All results use sequential integer change codes. Refresh the
baseline after approving compatible additions. Refreshing it for a breaking
change is allowed only on an intentional major-version branch after documenting
every reported break; normal `composer verify` never regenerates it.
`scripts/nimbus-bc-check.sh` runs `roave/backward-compatibility-check` against
the last released tag as a second, independent check.

## Template for a major release

Copy this section to the top of the file when preparing `X.0.0`, fill every
row from the `Deprecated` sections of the `X-1.*` changelog entries and from
the BC report of `scripts/nimbus-bc-check.sh`, then delete rows that do not
apply. Each removed API needs a before/after snippet.

```markdown
## From A.x to X.0

Requirements: PHP >= ?, pushinbr/pam-contracts ^?, PAM runtime >= ?, pushinbr/arcano ^?.

Automated part: `vendor/bin/rector process` with `NimbusSetList::UPGRADE_X_0` (if shipped).

### Removed APIs

| Removed | Deprecated since | Replacement |
| --- | --- | --- |
| `Nimbus\Foo::bar()` | A.3 | `Nimbus\Foo::baz()` |

Before:

    $foo->bar($x);

After:

    $foo->baz($x);

### Changed behaviour

- What changed, who is affected, how to detect it (log line, test failure, status code).

### Changed defaults and environment variables

| Setting | Old default | New default |
| --- | --- | --- |

### Checklist

1. Run the test suite on the last A.x release with deprecation notices enabled
   and fix every `Since pushinbr/nimbus` notice.
2. `pam composer require pushinbr/nimbus:^X.0`.
3. Clear `var/cache` and rebuild the Arcano schema cache.
4. Run the test suite and `composer api:compat` of your own packages, if any.
```

## From pam-http 2.2 to Nimbus 0.1

1. `pam composer remove pushinbr/pam-http` and `pam composer require pushinbr/nimbus`.
2. Replace namespaces: `Pam\Api\` becomes `Nimbus\` and `Pam\App` becomes `Nimbus\App`.
   Runtime classes keep their names (`Pam\Http\Request`, `Pam\Http\Response`).
3. Move route declarations to `routes.php` and load it with `$app->routes(__DIR__ . '/routes.php')`.
4. Use Arcano for the database (`ArcanoServiceProvider`); there is no other ORM integration.
