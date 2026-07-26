# Composer supply-chain gate

`pam supply-chain` produces a deterministic JSON decision from the committed
Composer manifest and lockfile. It checks code that Composer may execute as well
as package identity and provenance.

```bash
pam supply-chain . \
  --policy pam.supply-chain.json \
  --capabilities pam.capabilities.json \
  --output build/supply-chain.report.json
```

The online default invokes `composer audit --locked` with plugins and scripts
disabled. If Composer does not return valid advisory JSON, PAM fails closed and
does not produce a passing report. `--offline` is available for isolated builds,
but records advisory state `2` and a warning; it never represents the skipped
lookup as checked.

## Policy

```json
{
  "schemaVersion": 1,
  "denyScripts": false,
  "allowedScriptPrefixes": ["@php", "php ", "composer "],
  "allowedPlugins": ["pestphp/pest-plugin"],
  "allowedMaintainers": ["security@example.com"],
  "allowedLicenses": ["MIT", "BSD-3-Clause", "Apache-2.0"],
  "requireDistReference": true,
  "rejectAbandoned": true,
  "allowedCapabilities": [1, 2, 5]
}
```

An empty allowlist means that field has no additional PAM restriction. Composer
plugins are stricter: a plugin must be enabled by Composer's own
`config.allow-plugins` and, when configured, the PAM allowlist.

Scripts containing downloader/shell/elevation/removal patterns are critical even
when their prefix is allowed. Review every script as executable code.

## Integer report contract

Verdicts:

| Value | Meaning |
| ---: | --- |
| `1` | pass |
| `2` | review |
| `3` | fail |

Finding severity uses `1` information, `2` warning and `3` critical. Finding
kinds are sequential:

| Value | Finding |
| ---: | --- |
| `1` | Composer script |
| `2` | Composer plugin |
| `3` | maintainer |
| `4` | license |
| `5` | immutable source/dist provenance |
| `6` | security advisory |
| `7` | capability |
| `8` | abandoned package |

Advisory state is `1` when checked and `2` when explicitly skipped. A critical
finding makes the command exit with status `1`; warnings keep the report usable
but set verdict `2`. Invalid inputs, audit failures and policy/schema errors are
command errors.

The report includes the SHA-256 of `composer.lock`, package count, sorted
capability kinds and sorted findings. Attach it beside the CycloneDX SBOM,
signed bundle manifest and build provenance in CI.
