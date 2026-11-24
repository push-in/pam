# Changelog

All notable changes to `pushinbr/nimbus-rector` are documented here.

## Unreleased

### Added

- `NimbusSetList::LARAVEL_TO_NIMBUS`: class renames (Request/Response, FormRequest, JSON
  resources, HTTP client, `Cache` and `Hash` facades) and four rules:
  `LaravelRequestCallToNimbusRector`, `ResponseHelperToResponseParameterRector`,
  `RouteFacadeToRouterRector` and `AbortToHttpExceptionRector`, with fixture tests.
