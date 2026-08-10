# Use PAM as the project console

PAM is the only global executable required by an application. Composer,
Artisan, PAM Native, PAM Desktop, formatters, analyzers, generators, and release
gates are reached through the same contextual command surface.

## Daily workflow

```bash
pam doctor --fix
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

Explicit namespaces such as `pam mobile ...` and `pam desktop ...` are stable
advanced interfaces for CI and cross-project automation.

## Machine interfaces

These commands emit JSON without terminal decoration:

```bash
pam info --json
pam doctor --json
pam packages --json
pam commands --json
```

Each payload contains an integer `schema`. Automation must reject schemas it
does not understand and use the process exit code as the success contract.

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

See the generated [complete command catalog](cli-reference.md) and
[CLI extension contract](extending-cli.md).
