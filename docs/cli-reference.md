# PAM CLI reference

This file is generated from the CLI command catalog. Do not edit it manually.

## Project

- `pam new` — Create a project interactively.
- `pam init` — Create a project from a preset.
- `pam info` — Describe the active project.
- `pam doctor` — Validate or repair the active project.
- `pam support` — Create a bounded redacted support report.
- `pam clean` — Remove bounded project development artifacts.
## Develop

- `pam dev` — Start the contextual development session.
- `pam run` — Build and launch the active application.
- `pam logs` — Stream logs from the active application.
- `pam devices` — List connected development targets.
- `pam devtools` — Toggle contextual development tools.
- `pam console` — Open the application console.
- `pam commands` — List application and package commands.
## Generate

- `pam make:screen` — Generate a native screen.
- `pam make:component` — Generate a native component.
- `pam make:native-view` — Generate a native view bridge.
- `pam make:model` — Generate a Laravel model.
- `pam make:controller` — Generate a Laravel controller.
- `pam make:request` — Generate a Laravel form request.
- `pam make:resource` — Generate a Laravel API resource.
- `pam make:migration` — Generate a Laravel migration.
- `pam make:test` — Generate a Laravel test.
- `pam make:command` — Generate a Laravel console command.
- `pam make:job` — Generate a Laravel job.
## Ecosystem

- `pam packages` — List official PAM capabilities.
- `pam registry` — Verify signed plugin metadata and compatibility.
- `pam add` — Install an official capability.
- `pam remove` — Remove an official capability.
- `pam outdated` — Inspect available dependency updates.
- `pam composer` — Run Composer inside PAM.
- `pam artisan` — Run Laravel Artisan inside PAM.
## Quality

- `pam format` — Format project source.
- `pam lint` — Run formatting and static-analysis gates.
- `pam test` — Run Pest or PHPUnit inside PAM.
- `pam benchmark` — Run the contextual benchmark.
- `pam profile` — Capture contextual performance data.
## Ship

- `pam build` — Create a release build.
- `pam package` — Create a distributable package.
- `pam sign` — Validate native release signing.
- `pam release` — Validate and publish a release candidate.
- `pam release:verify` — Verify a Product release offline.
- `pam distribution:verify` — Verify signed clean-host distribution evidence.
- `pam distribution:sign` — Sign verified clean-host distribution evidence.
- `pam distribution:desktop-report` — Bind native Desktop trust proofs to an installer.
## Runtime

- `pam start` — Run a supervised server cluster.
- `pam octane:start` — Start Laravel Octane on the PAM runtime.
- `pam octane:status` — Inspect the PAM Octane master.
- `pam octane:reload` — Reload PAM Octane without downtime.
- `pam octane:stop` — Gracefully stop PAM Octane.
- `pam exec` — Execute a PHP script explicitly.
## Observe

- `pam inspect` — Inspect runtime capabilities.
- `pam routes` — List application routes.
- `pam diagnostics` — Capture runtime diagnostics.
- `pam timeline` — Export a bounded performance timeline.
- `pam top` — Stream live runtime metrics.
## Advanced

- `pam catalog` — Discover the versioned CLI contract.
- `pam mobile` — Use explicit PAM Native commands.
- `pam desktop` — Use explicit PAM Desktop commands.
- `pam completion` — Generate shell completion.
- `pam editor:install` — Install PAM language support in an editor.
- `pam self-update` — Install a cryptographically authorized PAM release.
- `pam docs:generate` — Generate the CLI reference.
