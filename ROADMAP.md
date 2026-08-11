# PAM roadmap

The roadmap communicates direction, not a delivery guarantee. Security,
correctness and compatibility regressions take priority over scheduled features.

## PAM Octane 1.0.3 community release

- Publish the runtime and `pushinbr/pam-octane` from one immutable tag.
- Certify fresh public installation on PHP 8.4 with Laravel 12 and 13.
- Collect reproducible compatibility reports from real applications.
- Publish clean-commit benchmark and 30-minute soak evidence.

## Near-term hardening

- Expand application fixtures for common authentication, queue, database and
  observability combinations.
- Add more fault-injection coverage around disconnects, worker crashes, reloads
  and partial upstream failure.
- Improve deployment examples for containers and common orchestrators.
- Track third-party packages that retain request state or assume a specific SAPI.

## Longer-term exploration

- Broaden cooperative database protocols without weakening transaction semantics.
- Improve cross-node cache invalidation and globally coordinated rate limits.
- Evaluate additional maintained PHP and Laravel release lines only through an
  executable compatibility contract.

Feature requests should explain the user problem, security boundary, operational
cost and measurable acceptance criteria. See `CONTRIBUTING.md`.
