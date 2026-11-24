# Changelog

All notable changes to `pushinbr/nimbus-rector` are documented here.

## Unreleased

## 0.1.0 - 2026-11-23

First Packagist release, split to `push-in/nimbus-rector`.

### Added

- `NimbusSetList::LARAVEL_TO_NIMBUS`: class renames (Request/Response, FormRequest, JSON
  resources, HTTP client, `Cache` and `Hash` facades) and four rules:
  `LaravelRequestCallToNimbusRector`, `ResponseHelperToResponseParameterRector`,
  `RouteFacadeToRouterRector` and `AbortToHttpExceptionRector`, with fixture tests.
