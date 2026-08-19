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
The `main` push trigger has no path filter, so documentation, governance and
workflow-only source publications receive the same 26-package evidence as
runtime changes. An executable regression test rejects restoration of a push
path filter.
Each package validates a committed lock when present, resolves the newest graph
allowed by its published constraints in a non-mutating preflight, installs that
graph only inside the disposable runner checkout, and then executes its tests
and declared static analysis on PHP 8.4 and 8.5. The 26 × 2 execution contract
matches both Native runtime lines. Each combination tests both the newest and
lowest dependency graph allowed by the package constraints, with an independent
non-mutating preflight before each installation. This prevents stale locks,
single-version runs and dishonest lower bounds from masking incompatibilities.
Composer plugins and scripts are disabled during both resolution paths. Each
resulting lock is then audited for known advisories and abandoned dependencies
with fail-closed policy before package code or static analysis is executed.
The inventory is bounded to 10 minutes and every package/PHP combination to 30
minutes; one hung resolver cannot consume a runner for the platform default.
Every successful combination uploads a schema 1 result tied to the actual PAM
checkout SHA, exact package checkout SHA and SHA-256 fingerprints of the latest
and lowest Composer locks that passed. Native-core tag runs additionally record
one candidate Git SHA across every dependent combination. A final job rejects missing/duplicate combinations, mixed commits,
identity/role drift or incomplete graph codes, then publishes one validated
`ecosystem-compatibility-evidence` artifact containing all 52 rows and 104 graph
executions with 30-day retention.
For a `pam-native-php` tag, every dependent job additionally checks out the
exact candidate ref and binds `pushinbr/pam-native` to it through an ephemeral
canonical Composer path repository. The installed lock must contain the exact
tag version and path-repository provenance before package tests can run, so the
gate does not accidentally certify dependents against the previously published
Native core or an identically numbered Packagist artifact during a re-run.
Runtime releases also pass their selected PAM ref into the reusable matrix.
This is essential for manual publication: `workflow_dispatch` certifies
`inputs.release_tag` rather than the branch commit from which the operator
started the workflow, matching every build and publish job in the release DAG.
Its concurrency key uses the same selected ref, preventing an unrelated `main`
push from cancelling an in-flight manual tag certification.
Its schema 1 catalog uses integer role codes: `1` core distribution, `2` device
capability, `3` product integration and `4` tooling. The inventory job compares
the catalog with every public `push-in/pam-native-*` repository, so adding or
removing a package without updating the compatibility authority fails closed.

For all 26 current repositories on both supported PHP series, the matrix checks
the Composer identity, PHP contract, PAM Native constraint where applicable, metadata validity,
dependency dry-run, real installation, declared tests and declared static
analysis on both constraint boundaries. The workflow also runs weekly to detect ecosystem drift between PAM
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

## 2026-08-18 Native outbound trace propagation

PAM Native source commit `e041449` added strict W3C version `00` propagation
for the host-owned HTTP client. The context is a dedicated value bound to one
exact HTTPS origin; case-insensitive generic `traceparent` and `tracestate`
headers are rejected, and Android/iOS independently revalidate the value and
origin before transmission. Follow-up `26bbc5f` adds negative contracts for
forged IDs, plaintext origins, embedded credentials, path-bearing origins,
cross-origin use and generic-header spoofing.

The first published-source CI run passed Swift/UIKit, Rust, PHP 8.4/8.5,
Android build/lint and instrumented renderer contracts on API 26 and API 36.
The Composer package split is deliberately not claimed as released until a new
versioned Native tag passes the newly connected global ecosystem gate and its
mirror publication workflow.

## 2026-08-18 bounded Native network timeline

PAM CLI commit `58f9d4d` added fail-closed Chrome/Perfetto export for Native
network events before Native began emitting the new integer kind. It accepts
only sequential method codes `1` through `5`, valid HTTP status codes and byte
counts within the Native request/response limits. Unknown snapshot fields,
including deliberately injected URL, label and header data in the tests, are
not copied into the trace.

PAM Native commit `bad65bd` added the corresponding Android and iOS diagnostics
to the existing eight-entry timeline. Each event retains only method/status
codes, request/response byte counts, duration and failure state. URLs, origins,
paths, queries, headers and bodies are never serialized. The source publication
passed [PAM CI](https://github.com/push-in/pam/actions/runs/32194299303),
[Laravel compatibility](https://github.com/push-in/pam/actions/runs/32194299485),
[required release gates](https://github.com/push-in/pam/actions/runs/32194299236)
and the full [26-package Composer matrix](https://github.com/push-in/pam/actions/runs/32194674415).

Native certification includes [Rust/PHP, Swift/UIKit, Android build and API
26/36 contracts](https://github.com/push-in/pam-native/actions/runs/32194384295),
plus explicit [Android ecosystem](https://github.com/push-in/pam-native/actions/runs/32194585232)
and [iOS ecosystem](https://github.com/push-in/pam-native/actions/runs/32194587117)
compilation of the official plugins. This is a source publication, not a new
Composer version: no tag or package release is claimed until the versioned
publication gate runs on that exact tag.

## 2026-08-18 fail-closed Native and Desktop tag releases

The bounded-network delivery exposed that PAM Native's aggregate Android and
iOS workflows did not automatically run for platform-source changes; they had
to be dispatched manually. Commit `9da8325` made source CI and both aggregate
plugin workflows reusable, expanded their automatic path coverage, and made
all three hard dependencies of the GitHub Release publisher alongside the
central Composer matrix. A checked-in executable contract prevents those
dependencies or path triggers from disappearing silently. The first push with
the new policy automatically started [source/device CI](https://github.com/push-in/pam-native/actions/runs/32195961606),
[Android ecosystem certification](https://github.com/push-in/pam-native/actions/runs/32195961609)
and [iOS ecosystem certification](https://github.com/push-in/pam-native/actions/runs/32195961623).

The same audit found that PAM Desktop tag publication, especially its API-only
path, could bypass the complete CI that protects `main`. Desktop commit
`bd8ec18` made that CI reusable and a dependency of both release paths, with an
executable workflow regression contract. Its [first policy-enforced source
run](https://github.com/push-in/pam-desktop/actions/runs/32196230253) covers
formatting, Clippy, workspace and Composer tests, PHP static analysis,
reproducible/installed host evidence, footprint limits and signed official
Collector interoperability.

No version tag was created to test these controls: doing so would itself be a
real publication. GitHub accepted and executed the reusable workflow syntax on
the source pushes, while the regression contracts verify the tag dependency
graph statically. The next legitimate tag will exercise that graph on its exact
immutable commit before either publisher can run.

## 2026-08-18 Desktop macOS and Windows source certification

Desktop commits `20395fd`, `f8c2cb2`, `0bd55e5`, `f367fe1` and `228ff9e`
introduced a native pre-release matrix and removed Linux-only dependency,
notification, portal, secret-store, fixture and async-I/O assumptions. Commit
`ac93248` aligned the embedded engine and its cryptography, JavaScript and
SQLite graph with Servo 0.5 and made the platform matrix a hard dependency of
both tagged-release paths. Commit `fd86e98` corrected the workflow to install
the declared Rust 1.88 compiler by action ref and verify it at runtime instead
of silently following the moving stable toolchain.

The published `servo-fonts 0.5.0` crate still borrowed a temporary Core
Foundation language string on macOS. Commit `264ceec` carries a 476 KiB,
MPL-licensed source override from the exact upstream revision, retains the
value through the native call and omits the 14 MiB upstream test-fixture
corpus. Its executable regression contract rejects restoration of the invalid
expression and records when the override may be removed.

The final [native platform run](https://github.com/push-in/pam-desktop/actions/runs/32201537932)
passed gateway/plugin tests, strict Clippy and compilation of the real Servo
host on macOS arm64 and Windows x64. The matching [Linux source and package
CI](https://github.com/push-in/pam-desktop/actions/runs/32201537939) also passed.
These runs certify source compilation; they do not claim supported installers,
signing, notarization, clean-machine launch or production updates on macOS and
Windows. No version tag or binary release was created.
