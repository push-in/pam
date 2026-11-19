# Use PAM as the project console

PAM is the only global executable required by an application. Composer,
Artisan, PAM Native, PAM Desktop, formatters, analyzers, generators, and release
gates are reached through the same contextual command surface.

## Daily workflow

```bash
pam doctor --fix
pam support --output pam-support.json
pam dev
pam make:component ProfileCard
pam format
pam lint
pam test
pam release --check
pam package
```

PAM discovers the nearest `pam.json` while walking toward the filesystem root.
The integer `type` selects the implementation: `1` API, `2` Native, `3`
Laravel, `4` Desktop, and `5` raw runtime. Legacy discovery from
`pam-native.json`, `artisan`, or Composer remains available during migration.

## Contextual commands

| Command | API/runtime | Laravel | PAM Native | Desktop |
| --- | --- | --- | --- | --- |
| `pam dev` | PHP hot reload | Laravel host | device hot reload | desktop host |
| `pam console` | app extension | Tinker | app extension | app extension |
| `pam make:*` | app extension | Artisan | screens/components/views | app extension |
| `pam format` | installed PHP formatter | Pint | PAM formatter | installed PHP formatter |
| `pam lint` | Composer/PHPStan | Composer/PHPStan | PAM formatter/PHPStan | Composer/PHPStan |
| `pam build` | production bundle | production bundle | release APK | desktop build |
| `pam package` | `tar.gz` + SHA-256 | `tar.gz` + SHA-256 | AAB + SHA-256 | desktop implementation |
| `pam diagnostics` | runtime snapshot | runtime snapshot | live Android/iOS Simulator snapshot | live development snapshot |
| `pam timeline snapshot.json` | Chrome/Perfetto timeline | Chrome/Perfetto timeline | Chrome/Perfetto timeline from bounded device events | Chrome/Perfetto counter sample from bounded host metrics |

Explicit namespaces such as `pam mobile ...` and `pam desktop ...` are stable
advanced interfaces for CI and cross-project automation.

`pam top --json` streams one bounded, versioned NDJSON object per sample from the
control plane `/diagnostics` contract. This is the automation interface; the
default terminal view and Prometheus integrations continue to use `/metrics`.

## Privacy-safe support reports

`pam support [path]` runs the structured Doctor audit and emits one bounded JSON
report to standard output. It does not read application file contents, copy
environment variables, capture network data, or create a cache. Absolute project
and home paths are replaced with `$PROJECT` and `$HOME`; the embedded diagnostic
payload has a SHA-256 digest so a support recipient can detect accidental changes.
`--manager` explicitly adds the versioned `pam monit --json` snapshot with its
own SHA-256 digest after the same path redaction. The privacy contract then marks
process metadata as included while continuing to exclude environment values,
network data and log contents. Failure to collect requested manager evidence
makes the overall report unsuccessful instead of silently omitting it.

Persistence is opt-in:

```bash
pam support . --output pam-support.json
pam support . --manager --output pam-support-manager.json
```

The output path must end in `.json`, must not already exist, and is created with
owner-only permissions on Unix. PAM refuses oversized Doctor output instead of
producing an unbounded report. Review every report before sharing it: installed
package names and toolchain diagnostics can still reveal project metadata even
when paths and common secret-bearing sources are excluded.

CI exercises this contract against both a direct Runtime target and a generated
Server/Native/Desktop Product workspace containing adversarial source and
environment secrets. It recomputes the embedded diagnostic digest, checks the
512 KiB bound and owner-only persisted mode, verifies redaction, and proves that
an existing report cannot be overwritten. These reports reuse the seven-day
Doctor evidence artifact instead of creating another retained CI bundle.

## Private manager dashboard

`pam dashboard [FILE.html]` creates a dependency-free, read-only HTML snapshot
of every managed application. The default output is `pam-dashboard.html`; an
explicit path can also be supplied with `--output FILE.html`.

The snapshot includes application name and kind, textual process state, worker
count, aggregate resident memory, task count, and resource-warning state. It
excludes commands, paths, environment values, network data, and log contents.
It contains no JavaScript or external assets and adapts to light, dark,
high-contrast, reduced-motion, narrow-screen, keyboard, and screen-reader use.

Dashboard files must end in `.html`, are bounded to 2 MiB, and are created with
owner-only permissions on Unix. PAM never overwrites an existing snapshot:

```bash
pam dashboard manager-health.html
```

This is a local point-in-time flight recorder, not a network service. Review it
before sharing and use a new filename whenever fresh evidence is needed.

While `pamd` is running, it records one private resource sample per managed
application every 60 seconds. `pam monit:history [NAME] [--limit N] [--json]`
reads the newest samples; `--record` captures an explicit sample for incident
response and deterministic automation. Each application retains at most 120
samples in its own owner-only JSON record, so a busy or long-lived manager cannot
grow one unbounded history file. Deleting an application deletes this history.

History contains timestamps, integer state and alert codes, workers, aggregate
RSS, and task counts. It contains no commands, paths, environment values,
network data, or logs. The dashboard summarizes sample count, peak RSS, and the
RSS direction in text; exact samples remain available through the CLI table or
versioned JSON contract.

### Authenticated live local view

`pam dashboard:start --token-file TOKEN` starts the read-only dashboard as a
detached local service on `127.0.0.1:9615`. The token file must be a bounded,
regular, non-symlink file with 32–256 printable ASCII characters and owner-only
permissions. PAM reads it once, persists only its SHA-256 digest in private
manager state, and never places the token in arguments, URLs, status, HTML, or
logs. Override the address with `--listen 127.0.0.1:PORT`; non-loopback binds and
port zero fail closed.

```bash
umask 077
openssl rand -hex 32 > pam-dashboard.token
pam dashboard:start --token-file pam-dashboard.token
pam dashboard:status --json
pam dashboard:stop
```

Browsers can authenticate through HTTP Basic with user `pam` and the token as
password. Automation can send `Authorization: Bearer TOKEN`. `/` renders fresh
manager state on every authenticated GET and `/health` returns a versioned
integer-coded health envelope. Responses disable caching, framing, referrers,
MIME sniffing, scripts, external assets, and form submission through headers and
CSP. Refresh is explicit so screen-reader and keyboard context is not reset by a
timer. `status` never exposes credential material; `stop` removes runtime state
and the stored digest.

## Machine interfaces

These commands emit JSON without terminal decoration:

```bash
pam info --json
pam doctor --json
pam commands --json
pam catalog --json
```

Each payload contains an integer `schema`. Automation must reject schemas it
does not understand and use the process exit code as the success contract.
`pam doctor --json` additionally reports the resolved target, sequential integer
result and project-type codes, relevant manifest paths, the measured footprint
of every regenerable build directory, and an exact remediation/verification
pair under `nextActions`. Result code `1` is healthy and `2` needs attention;
action code `1` runs the healthy target, `2` repairs a recognized project, and
`3` requests manual inspection when no safe automatic repair exists. Consumers
should execute the separate `arguments` array instead of parsing display-only
command text.

`pam catalog --json` is the versioned discovery authority shared by help,
shell completion and the generated reference. Each command exposes its name,
summary, `supportsJson` capability, presentation-only group label and a backed
integer `groupCode`: `1` Project, `2` Develop, `3` Generate, `4` Ecosystem,
`5` Quality, `6` Ship, `7` Runtime, `8` Observe and `9` Advanced. Tooling can
therefore discover structured commands without scraping terminal output.
`pam catalog --schema` prints the embedded strict Draft 2020-12 contract for
offline validation; it limits command count and field sizes, rejects unknown
properties, and constrains names and group codes.
`pam catalog --validate catalog.json [--json]` performs the same bounded checks
inside PAM: it accepts only a regular non-symlink file up to 256 KiB, rejects
unknown/duplicate commands, verifies name and summary limits, and requires each
integer group code to match its presentation label.
`pam catalog --compat baseline.json candidate.json [--json]` validates both
documents and exits with `1` when the candidate removes a command (`changeCode`
`1`), moves it to another group (`2`), or withdraws existing JSON support (`3`).
Additive commands and presentation-only summary changes remain compatible. The
report order follows the baseline, making CI evidence deterministic.
`pam catalog --compat-schema` prints the strict Draft 2020-12 schema for this
report, including the invariant that compatible results have no changes and
incompatible results have at least one enum-backed change.
CI generates the candidate catalog and embedded schemas from the compiled
binary, consumes the saved catalog through this validator, compares it against
the committed `docs/contracts/cli-catalog-v1.json` baseline, independently
cross-checks every schema code/label pair, seals all JSON documents with SHA-256,
and retains the `pam-cli-catalog-contract-<commit>` evidence artifact for seven
days.

Any command can request a structured error envelope without changing its normal
success output:

```bash
pam --json-errors build
PAM_ERROR_FORMAT=json pam doctor
```

CLI validation, runtime, and server failures then write one JSON object to
standard output containing `schema`, the sequential integer `errorCode`,
`message`, `remediation`, `verificationCommand`, and `exitCode`. Human errors
show the same stable identifier as `PAM-E001` through `PAM-E008`, a suggested
fix, and the exact command that verifies the repaired state. Existing
scripts that do not opt in retain the human-readable error stream; application
process exit codes remain application-owned.

## PHP and Composer compatibility

`pam composer` uses the verified Composer toolchain. Composer bin proxies run
inside an isolated Embed lifecycle so PHAR-scoped dependencies do not collide
with application classes. When a tool starts `PHP_BINARY`, PAM accepts the
relevant PHP CLI options such as `-c` and `-d` and remains the child PHP binary.

Laravel commands retain normal Artisan arguments, streams, environment, and
exit codes:

```bash
pam artisan migrate
pam make:model Post --migration
pam console
```

Nimbus projects have native generators for Arcano entities, controllers,
typed `Input` requests, Resources, migrations, services, repositories, jobs,
factories, seeders and API tests:

```bash
pam make:model User
pam make:controller UserController
pam make:request StoreUserRequest
pam make:resource UserResource
pam make:service CreateUserService
pam make:repository UserRepository
pam make:migration CreateUsersTable
pam make:job SendWelcomeEmail
pam make:factory UserFactory
pam make:seeder DatabaseSeeder
pam make:test UserApiTest
pam make:admin Product          # app/Admin/ProductAdmin.php (Nimbus admin panel)
```

Generators require a bounded PascalCase name, create missing application
directories and refuse to overwrite existing files. Generated controllers use
named methods, ready for `[UserController::class, 'index']` route handlers.
`pam make:view orders.index` creates `resources/views/orders/index.nim.php`,
inside `<x-layout>` when the application has that component.

## Nimbus starter kits

```bash
pam init my-api --template api   # JSON API (the default)
pam init my-app --template web   # server-rendered pages with login
```

The web starter is a Nimbus application with sessions (files in
`storage/sessions`), CSRF on every form, `resources/views/**/*.nim.php` views
with the layout as the `<x-layout>` component, login, registration, logout,
password reset by e-mail (`PasswordResets`, `MAILER_DSN`) and a dashboard that
only signed-in users reach. Users are an Arcano entity (`app/Models/User.php`)
hashed with `Nimbus\Security\Hash`; texts come from `lang/pt_BR` and `lang/en`
(`/locale/en` switches the session). `bootstrap/app.php` builds the application
for `index.php`, `bin/migrate` and the tests, and reads `.env`, which `pam init`
creates with a random `APP_KEY`. No JavaScript is required.

```bash
cd my-app
pam composer install
pam exec bin/migrate          # SQLite in storage/database.sqlite by default
pam test                      # feature tests on in-memory SQLite
pam dev index.php
```

## Checking views

`pam views:check` compiles every `resources/views/**/*.nim.php` template of the
project with the installed `vendor/pushinbr/nimbus/bin/nimbus-views` and
prints each problem as `file:line: message`. It exits with `0` when every
template compiles and `1` otherwise, so it can gate CI:

```bash
pam views:check
pam views:check resources/admin-views 'App\Admin\Views'
```

The optional arguments are the views directory and the namespace of class
components (default `App\Views`).

## Deploying to your own server

`pam deploy:remote` ships a Composer application to a Linux server that has
`pam`, `rsync` and an SSH key for the user, using the local `ssh` and `rsync`:

```bash
pam deploy:remote deploy@app.example.com --init   # first time
pam deploy:remote deploy@app.example.com          # every release
pam deploy:remote deploy@app.example.com --dry-run
```

Each run copies the project into `ROOT/releases/<UTC timestamp>` (without
`vendor/`, `.git/`, `node_modules/`, `.pam/`, `.env` and `storage/`), links
`ROOT/shared/.env` and `ROOT/shared/storage` into it, runs
`pam composer install --no-dev --optimize-autoloader`, runs `bin/migrate` when
the application has one, and activates the release. The first release is
started with `pam up`; the next ones go through the readiness-gated
`pam deploy`, which keeps the previous release online when the new one fails
to start. `ROOT/current` points to the active release, the last five releases
are kept, and `pam rollback NAME` (with `PAM_MANAGER_STATE_DIR=ROOT/state` and
`PAM_MANAGER_RUNTIME_DIR=ROOT/state/run`) returns to an earlier one.

`--init` also creates `ROOT/shared/.env` from `.env.example` when the server
has none (a new `APP_KEY`, `SESSION_SECURE_COOKIE=true`, `PAM_ENV=production`),
and installs and starts `~/.config/systemd/user/pam-<name>.service`. The unit
runs the application's own `pamd` with its state in `ROOT/state`, which brings
the saved application back after a reboot; `loginctl enable-linger` keeps it
running without a login session.

Options: `--path` (`ROOT`, absolute or relative to the remote home; default
`pam-apps/<name>`), `--name` (default: `name` in `pam.json`), `--workers`,
`--health-check-url`, `--keep`, `--pam` (the executable on the server) and
`--dry-run`, which prints the commands. Defaults can live in `pam.json`:

```json
{
    "deploy": {
        "host": "deploy@app.example.com",
        "path": "/srv/shop",
        "workers": 4,
        "healthCheckUrl": "http://127.0.0.1:3000/login"
    }
}
```

The server resolves Composer packages itself, so path repositories that only
exist on the development machine must be published or removed first.

See the generated [complete command catalog](cli-reference.md) and
[CLI extension contract](extending-cli.md).
