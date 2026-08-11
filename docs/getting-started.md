# Start with PAM

PAM is the single command-line interface for the whole ecosystem: HTTP APIs,
Laravel, realtime services, PAM Native mobile applications, PAM Desktop, data
packages, plugins, development tools, and production builds.

You install PAM once and use `pam` for the rest of the project lifecycle.

## Install

```bash
curl --proto '=https' --tlsv1.2 --fail --silent --show-error --location \
  https://github.com/push-in/pam/releases/latest/download/install.sh | sh
```

```bash
pam --version
pam doctor
```

Official releases include a private PHP runtime and acquire a verified Composer
PHAR when required. A separately installed PHP CLI, Composer, Rust, Gradle, or
Xcode is not needed merely to explore PAM. Platform build tools are diagnosed
when a project actually targets that platform.

For Android projects, `pam doctor --fix` installs PAM's checksum-verified PHP
runtimes and native engines. Application developers do not need a Rust
toolchain unless they intend to rebuild the engine itself.

## Create a project interactively

```bash
pam
```

Or choose a preset explicitly:

```bash
pam init my-api --template api
pam init my-laravel-app --template laravel
pam init my-native-app --template mobile
pam init my-desktop-app --template desktop
```

Every generated project contains `pam.json`. This lets the CLI select the right
implementation when you use the same short commands everywhere:

```bash
cd my-native-app
pam doctor
pam dev
pam test
pam build
```

For a Native project, install `.pam` syntax highlighting, diagnostics,
completion, hover help, and format-on-save in VS Code with:

```bash
pam editor:install vscode
```

Neovim and Helix users receive safe managed configuration without replacing
their existing editor setup:

```bash
pam editor:install neovim
pam editor:install helix
```

## Ship and update

The same distribution command creates the platform-appropriate artifact. API,
Laravel, and raw runtime projects receive a versioned `tar.gz`; PAM Native
produces signed Android AAB/APK artifacts or an exported iOS IPA. Every binary
receives a neighboring SHA-256 file.

```bash
pam release --check
pam package
pam self-update --check
```

`pam release` runs doctor, lint, and tests before packaging. Native releases
also require the platform signing environment before Gradle creates the AAB or
Xcode exports the IPA.

CI and editor integrations can consume project discovery and health without
parsing terminal decoration:

```bash
pam info --json
pam doctor --json
```

Legacy namespaced commands such as `pam mobile build` remain supported for CI
and explicit cross-project automation.

## Add ecosystem capabilities

```bash
pam packages
pam add maps
pam add auth
pam remove maps
```

PAM queries package metadata and performs a non-mutating Composer preflight
before it changes the manifest or lockfile.

## Automation

Interactive flows always have deterministic command forms for CI:

```bash
pam init my-native-app \
  --template mobile \
  --no-interaction
```

Use `pam help <command>` for focused usage. Commands intended for tooling expose
stable exit codes. `pam info --json` provides the stable project-discovery
payload; generated command documentation is checked in CI so the website and
binary cannot silently drift.
