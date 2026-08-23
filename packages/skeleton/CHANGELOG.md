# Changelog

All notable changes to PAM Skeleton are documented in this file.

## 2.0.1 - 2026-08-22

- Require the permanent `pushinbr/pam-http-psr` interoperability package.
- Require `pushinbr/pam-http-testing` for the generated in-process test suite.
- Preserve PHP 8.5 as the generated application's default runtime.

## 2.0.0 - 2026-08-21

- Make PHP 8.5 the default application runtime.
- Replace `pushinbr/pam-api` with `pushinbr/pam-http` 2.x.
- Replace `pushinbr/pam-psr-bridge` with `pushinbr/pam-psr`.
- Require canonical PAM Socket and PAM Testing releases without legacy
  dependencies.
- Standardize the PAM-first Composer installation guide.

## 1.0.2 - 2026-08-20

- Preserve the original starter before the Composer ecosystem migration.
