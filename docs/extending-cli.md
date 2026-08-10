# Extend the PAM CLI

Applications and Composer packages can expose PHP commands without installing
another global executable. PAM validates command names, canonicalizes scripts,
rejects paths outside the project, rejects duplicates, and executes the script
through the same PHP Embed lifecycle.

## Application commands

Add a `commands` object to `pam.json`:

```json
{
  "schema": 1,
  "type": 1,
  "name": "billing-api",
  "commands": {
    "billing:reconcile": {
      "script": "bin/reconcile.php",
      "description": "Reconcile pending invoices"
    }
  }
}
```

```bash
pam commands
pam billing:reconcile --since=yesterday
```

Definitions may use a string when no custom description is needed:

```json
{"commands":{"app:warm":"bin/warm.php"}}
```

## Package commands

A Composer package registers commands under `extra.pam.commands`. Script paths
are relative to the installed package root:

```json
{
  "name": "vendor/pam-inspector",
  "extra": {
    "pam": {
      "commands": {
        "inspector:snapshot": {
          "script": "bin/snapshot.php",
          "description": "Capture an application snapshot"
        }
      }
    }
  }
}
```

Command names use lowercase ASCII letters, integers, `:`, `-`, or `_`, start
with a letter/integer, and contain at most 96 bytes. A package cannot shadow a
PAM built-in or another registered command; duplicate registrations fail the
doctor and command discovery gates.

Use `pam commands --json` to drive launchers and editor integrations without
parsing human-readable output.
