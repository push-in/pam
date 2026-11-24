# Nimbus versioning, deprecation and support policy

This policy takes effect with Nimbus 1.0.0. Until then the 0.x line follows
the same rules except that a minor release may still change public API, always
recorded in `CHANGELOG.md` and `UPGRADE.md`.

## Semantic Versioning

Nimbus (`pushinbr/nimbus`) uses `MAJOR.MINOR.PATCH`:

| Release | May contain | Never contains |
| --- | --- | --- |
| Patch `1.4.x` | bug and security fixes, documentation, performance work with the same behaviour | new public API, deprecations, removals |
| Minor `1.x.0` | new public API, new optional integrations, deprecations | removals, signature changes that break callers or implementers |
| Major `x.0.0` | removal of APIs deprecated during the previous major, raised PHP or `pam-contracts` minimum | removals that skipped the deprecation cycle |

Nimbus versions independently from the PAM runtime and from Arcano. Each Nimbus
release states the PAM runtime and `pushinbr/arcano` ranges it is tested against
in `composer.json` (`conflict`) and in the changelog entry.

## What is public API

The public API is every class, interface, enum and member in `src/` that is
not marked `@internal`, plus the functions of `src/helpers.php`. The inventory
is generated, not maintained by hand:

- `api-surface.json`: the reflection snapshot (333 symbols at the time of
  writing) that `composer api:compat` compares on every pull request;
- [`PUBLIC-API.md`](PUBLIC-API.md): the same snapshot as a readable reference
  (`composer docs:generate`).

Covered: class and member names, parameter names (named arguments are part of
the contract), parameter and return types, default values, constants, enum
cases and their backing values, interface methods, `final`/`readonly`
modifiers, the HTTP behaviour documented in `docs/nimbus` (status codes,
`problem+json` bodies, `ProblemCode` integers, header names) and the
environment variables the documentation lists.

Not covered:

- anything marked `@internal` and every trait (37 symbols today: pipeline plumbing, template
  compiler, codecs, profiler recorders, admin internals), traits, `private`
  members and `protected` members of `final` classes;
- the exact text of exception and log messages;
- the format of files under `var/cache` and of internal job payloads written
  by one release and read by the same release;
- classes under `tests/` and `phpstan/`, except the documented PHPStan
  extension entry point `phpstan/extension.neon`.

Interfaces meant to be implemented by applications (for example
`Cache\Store`, `Jobs\JobQueue`, `Auth\Authenticator`, `Health\HealthCheck`)
get a new method only in a major release, or in a minor release together with
a default implementation in an abstract base class or a separate interface.

Inventory by namespace (public symbols in `api-surface.json` / `@internal` or trait):

| Namespace | Public | Internal |
| --- | ---: | ---: |
| `Nimbus` | 8 | 7 |
| `Nimbus\Admin` | 5 | 3 |
| `Nimbus\Auth` | 45 | 3 |
| `Nimbus\Broadcasting` | 12 | 0 |
| `Nimbus\Cache` | 9 | 0 |
| `Nimbus\Config` | 4 | 0 |
| `Nimbus\Console` | 1 | 0 |
| `Nimbus\Container` | 4 | 1 |
| `Nimbus\Database` | 18 | 5 |
| `Nimbus\Events` | 5 | 1 |
| `Nimbus\Health` | 4 | 0 |
| `Nimbus\Http` | 27 | 0 |
| `Nimbus\Idempotency` | 3 | 0 |
| `Nimbus\Jobs` | 20 | 0 |
| `Nimbus\Lifecycle` | 1 | 0 |
| `Nimbus\Mail` | 3 | 0 |
| `Nimbus\Middleware` | 18 | 0 |
| `Nimbus\Notifications` | 36 | 1 |
| `Nimbus\Observability` | 2 | 0 |
| `Nimbus\OpenApi` | 6 | 0 |
| `Nimbus\Pagination` | 2 | 0 |
| `Nimbus\Profiler` | 7 | 6 |
| `Nimbus\RateLimit` | 3 | 0 |
| `Nimbus\Redis` | 10 | 2 |
| `Nimbus\Resilience` | 4 | 0 |
| `Nimbus\Routing` | 1 | 0 |
| `Nimbus\Runtime` | 2 | 0 |
| `Nimbus\Scheduling` | 7 | 0 |
| `Nimbus\Security` | 5 | 0 |
| `Nimbus\Session` | 9 | 1 |
| `Nimbus\Storage` | 8 | 0 |
| `Nimbus\Tenancy` | 3 | 0 |
| `Nimbus\Testing` | 10 | 1 |
| `Nimbus\Transactions` | 1 | 0 |
| `Nimbus\Translation` | 5 | 0 |
| `Nimbus\Validation` (with `Attributes`) | 14 | 3 |
| `Nimbus\View` | 11 | 3 |

Marking a public symbol `@internal` is itself a breaking change after 1.0 and
follows the deprecation cycle below.

## Deprecation cycle

A public API is removed only after it has been deprecated for at least one
minor release and only in the next major release:

1. **Deprecate in a minor release** `1.N.0`. The symbol gets a PHPDoc tag that
   names the replacement and the removal version (hypothetical example):

   ```php
   /**
    * @deprecated since 1.3, use RouteRegistrar::auth() instead; removed in 2.0.
    */
   public function authenticated(): self
   {
       Deprecation::trigger('1.3', '%s() is deprecated, use %s instead.', __METHOD__, 'RouteRegistrar::auth()');

       return $this->auth();
   }
   ```

   `Nimbus\Deprecation::trigger()` calls `trigger_deprecation('pushinbr/nimbus', '1.3', ...)`
   from `symfony/deprecation-contracts` when that package is installed, and
   otherwise raises a silenced `E_USER_DEPRECATED` with the same message
   (`Since pushinbr/nimbus 1.3: ...`). Nimbus core keeps no production
   dependency besides `pushinbr/pam-contracts`. The notice is emitted once per
   call, never at boot only because the class was loaded.
2. **Keep it working** through every remaining `1.x` release, covered by the
   same tests as before plus a test that asserts the notice.
3. **Record it**: the `Deprecated` section of the changelog entry and a row in
   `UPGRADE.md` for the next major, with the replacement code side by side.
4. **Remove it in `2.0.0`**, not earlier. A deprecation added in the last
   minor before a major still waits for that major only if it shipped in at
   least one minor release; otherwise it moves to the major after.

PHPStan users see the `@deprecated` tags through `phpstan/phpstan-deprecation-rules`;
PHPUnit reports the runtime notices with `displayDetailsOnTestsThatTriggerDeprecations`.

## Support and LTS

| Line | Bug fixes | Security fixes |
| --- | --- | --- |
| Latest minor of the current major | until the next minor | until the next minor |
| `1.x` (LTS) | until 2.0.0 is released | 18 months after the release of 2.0.0, on the last `1.x` minor |
| `0.x` | none after 1.0.0 | none after 1.0.0 |

Security fixes for 1.x land as patch releases of the last 1.x minor; upgrading
within 1.x is expected to be a `composer update` with no code change. The PHP
versions supported by each line follow the [PHP version policy](../../../docs/php-version-policy.md):
a minor release never drops a PHP version that still receives upstream security
support.

Vulnerabilities are reported as described in [SECURITY.md](../SECURITY.md).

## Enforcement

- `composer api:compat` (`bin/api-compat`) runs in the required Nimbus gate and
  fails on any public symbol, member or signature change against
  `api-surface.json`. Compatible additions are reviewed and committed with
  `bin/api-compat update`.
- `scripts/nimbus-bc-check.sh` (workflow `Nimbus backward compatibility`) runs
  [`roave/backward-compatibility-check`](https://github.com/Roave/BackwardCompatibilityCheck)
  against the newest `v*` tag of the split repository `push-in/nimbus` on
  every pull request that touches `packages/nimbus`. A reported break blocks
  the merge unless the pull request targets a major-version branch. During
  0.x, an accepted break is listed in `scripts/nimbus-bc-baseline.xml` and in
  the changelog, and the list is emptied after the next tag.
- A release follows [RELEASE.md](../RELEASE.md); a major release also fills
  in [UPGRADE.md](../UPGRADE.md) from its template.
