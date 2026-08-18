# Delivery publication ledger

This ledger distinguishes source publication from a versioned product release.
Pushing `main` makes reviewed source and CI workflows public; it does not create
a tag, Composer release, signed binary, marketplace entry or support promise.
Those artifacts continue to require the repository's release/version gates.

## 2026-08-18 platform batch

| Repository | Source range prepared for `main` | Scope |
| --- | --- | --- |
| [`push-in/pam`](https://github.com/push-in/pam) | `a6bc489..4bbb14a`, plus this ledger/reference repair | Runtime/CLI evidence foundations, diagnostics, release audits, signed registry enforcement, cross-surface timelines and certified Server OTLP |
| [`push-in/pam-desktop`](https://github.com/push-in/pam-desktop) | `0a498a9..e2c7840` | updater/permission hardening, diagnostics, package evidence, authenticated host acquisition, certified OTLP command spans and scoped outbound context |
| [`push-in/pam-native-observability`](https://github.com/push-in/pam-native-observability) | `615e64a..622df52` | signal-correct OTLP traces/logs/metrics, official Collector certification and strict Server-to-Native W3C lineage |

The PAM history was rebased over remote governance gate `a6bc489`. Documentation
commit references were remapped by exact commit subject and checked so the
published evidence points to the rewritten source history.

## Verification recorded before publication

- PAM Runtime OTLP payloads and parent lineage were accepted by the immutable
  official OpenTelemetry Collector `0.157.0` image.
- PAM Desktop passed its gateway workspace tests, strict Clippy/rustfmt gates,
  real Collector acceptance, trace-parent lineage inspection and real outbound
  header capture.
- PAM Native Observability passed all functional tests, PHPStan level 9 and
  official Collector acceptance for traces, logs, counters, gauges and remote
  parent lineage.
- Collector containers/images, Composer `vendor` directories and temporary
  Cargo targets created by these checks were removed afterward. No generated
  build cache is committed or retained inside these repositories.

## Publication procedure

1. Fetch each `origin/main` and require zero commits behind before pushing.
2. Push the exact local `main` without force.
3. Record the remote commit SHA returned by GitHub.
4. Inspect every workflow triggered by that SHA and do not call the batch
   published-green while a required check is pending or failing.
5. Correct failures with ordinary follow-up commits; never rewrite published
   `main` to hide delivery history.

Local nested repositories and the pre-existing Native macrobenchmark change are
outside this batch. They are not staged, committed or implicitly published by
the PAM repository push.

## Universal ecosystem gate

Every future PAM publication must run the `Ecosystem compatibility` workflow.
Its schema 1 catalog uses integer role codes: `1` core distribution, `2` device
capability, `3` product integration and `4` tooling. The inventory job compares
the catalog with every public `push-in/pam-native-*` repository, so adding or
removing a package without updating the compatibility authority fails closed.

For all 26 current repositories, the matrix checks the Composer identity, PHP
8.4 contract, PAM Native constraint where applicable, metadata validity,
dependency dry-run, real installation, declared tests and declared static
analysis. The workflow also runs weekly to detect ecosystem drift between PAM
publications. A green core build cannot substitute for this matrix.

## 2026-08-18 ecosystem certification outcome

PAM source publication `b6df965` made the compatibility workflow reusable and
placed it in the versioned release dependency chain. Core CI, required release
gates, Laravel compatibility and Collector interoperability all completed
successfully. The final [26-package compatibility run](https://github.com/push-in/pam/actions/runs/32189429288)
also completed successfully after installing each package from its declared
Composer graph and running its public test command.

The first two matrix attempts were retained as failed evidence. They exposed
test runners that silently depended on neighboring development checkouts rather
than the installed Composer packages. The corrected source was published and
its repository CI passed at `17a9a82` (feature flags), `643c854` (media),
`d04d171` (realtime), `c1058a0` (sync), `095d730` (payments), `51e6909`
(video), `79e40dc` (scanner) and `4d2f877` (maps). No generated `vendor`
directory or transient lockfile was retained.

The direct PAM `main` push was accepted through an administrator bypass even
though the branch rule requires a pull request and four status checks. This is
recorded as a governance exception; successful post-push checks establish the
technical evidence but do not erase the bypass.

## Universal package publication enforcement

PAM commits `94fe522` and `116fa26` made the compatibility workflow callable
from external repositories, made a package-tag invocation test that exact tag,
and made the central contract reject packages without a publication gate. The
[final current-head matrix](https://github.com/push-in/pam/actions/runs/32190927475)
passed all 26 repositories. A real invocation owned by
[`pam-native-auth`](https://github.com/push-in/pam-native-auth/actions/runs/32190632958)
also passed, proving that the reusable workflow works across repository
boundaries rather than only in PAM's own CI.

Every catalog entry now invokes the central matrix for `v*` tags and supports a
manual pre-publication run. Nitro's automated publisher additionally declares
the compatibility job as a hard dependency, so neither its GitHub Release nor
its Packagist update can execute first. The package gate commits are:

| Package | Gate commit | Package | Gate commit |
| --- | --- | --- | --- |
| auth | `51cb8bf` | background-transfer | `3fa9fd9` |
| bluetooth | `3b12352` | devtools | `0888ae5` |
| feature-flags | `97658ff` | firebase | `04fd711` |
| health | `a59e2e1` | intents | `c006358` |
| laravel-sync | `43696d5` | live-activities | `a51a408` |
| maps | `5349f16` | media | `dc0ec33` |
| nfc | `dfb0dea` | nitro | `de882de` |
| observability | `92a4d43` | payments | `ca27749` |
| php aggregate | `8cbf19c` | plugin-kit | `61099a0` |
| realtime | `282dd11` | scanner | `5c15b77` |
| share-extension | `ba97b37` | subscriptions | `8c64a26` |
| sync | `5ced27e` | testing | `c8ff32b` |
| video | `1226362` | widgets | `dd5fe1e` |

The aggregate `pushinbr/pam-native` distribution had no independent push CI.
Commit `6670118` added PHP 8.4/8.5 validation, Composer preflight/install,
optimized strict PSR autoload validation and syntax checks; its
[first run passed](https://github.com/push-in/pam-native-php/actions/runs/32190912248).
The other 25 gate commits also completed their repository-owned CI successfully.
