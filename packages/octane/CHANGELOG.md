# Changelog

All notable changes to PAM Octane will be documented here.

## 1.0.3 - 2026-08-11

- Bridge PAM's native HTTP transport directly to `Laravel\Octane\Worker`.
- Preserve streamed responses, downloads, cookies, headers and output buffers.
- Add package discovery and the `pam:octane` Artisan command.
- Add configurable request and response safety limits.
- Add route-aware worker pools, bounded response caching and route metrics.
- Certify PHP 8.4 with Laravel 12 and 13.
- Add native runtime smoke, release-install smoke and community support contracts.
