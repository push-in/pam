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
- `pam logs` — Query or follow managed application logs.
- `pam daemon` — Manage the per-user PAM supervisor.
- `pam scale` — Change a managed application's worker count.
- `pam save` — Save the desired managed process list.
- `pam resurrect` — Restore the saved managed process list.
- `pam startup` — Generate or install the Linux user service.
- `pam monit` — Show managed process health and capacity.
## Observe

- `pam monit:history` — Inspect bounded process resource history.
## Develop

- `pam dashboard` — Create a private manager health dashboard.
## Observe

- `pam dashboard:start` — Start the authenticated local live dashboard.
- `pam dashboard:status` — Inspect the local live dashboard service.
- `pam dashboard:stop` — Stop the local live dashboard service.
## Develop

- `pam apply` — Reconcile applications from pam.toml.
- `pam config:check` — Validate a pam.toml ecosystem contract.
## Ship

- `pam deploy` — Activate a readiness-gated release.
- `pam deploy:history` — Inspect bounded deployment history.
- `pam rollback` — Restore a previous healthy release.
- `pam traffic:start` — Start a weighted release ingress.
- `pam traffic:set` — Change candidate traffic atomically.
- `pam traffic:promote` — Promote candidate to stable.
- `pam traffic:abort` — Abort candidate traffic.
## Observe

- `pam traffic:status` — Inspect release traffic state.
## Ship

- `pam traffic:evaluate` — Gate a candidate using live evidence.
- `pam traffic:stop` — Stop a release traffic ingress.
## Develop

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
- `pam up` — Start an environment-aware, health-supervised application.
- `pam ps` — List managed PAM applications.
- `pam status` — Inspect managed application health.
- `pam describe` — Describe a managed application.
- `pam reload` — Reload a managed application without downtime.
- `pam restart` — Restart a managed application.
- `pam stop` — Gracefully stop a managed application.
- `pam delete` — Remove a stopped application from PAM.
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
