# PAM API security policy

Do not report vulnerabilities in a public issue. Use GitHub private
vulnerability reporting on the [PAM repository](https://github.com/push-in/pam/security/advisories/new).

Include the affected version, minimal reproduction, impact and any known
mitigation. Do not include production credentials, personal data or active
third-party targets.

Security support covers maintained releases listed by the root
[security policy](https://github.com/push-in/pam/blob/main/SECURITY.md). The PAM
team will acknowledge a valid private report, coordinate remediation and
publish an advisory after a fixed release is available.

Debug profilers must remain disabled in production. Signing keys, database
credentials and bearer tokens must come from a secret manager and must never be
included in logs, traces, diagnostics or issue reports.
