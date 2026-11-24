# pushinbr/nimbus-rector

Rector rules that rewrite the mechanical part of a Laravel application for
[Nimbus](../nimbus/README.md). The rest of the port (Eloquent models to Arcano
entities, middleware, service providers, config) is manual and described in the
[Laravel migration guide](../../docs/nimbus/migracao-laravel.md).

## Start here

Install PAM first (`curl -fsSL https://push-in.github.io/pam/install.sh | sh`), then
add the rules to the application being ported. Until the package is published on
Packagist, point Composer at this directory:

```bash
pam composer config repositories.nimbus-rector path /path/to/pam/packages/nimbus-rector
pam composer require --dev pushinbr/nimbus-rector:@dev
```

`rector.php` at the project root:

```php
<?php

use Nimbus\Rector\Set\NimbusSetList;
use Rector\Config\RectorConfig;

return RectorConfig::configure()
    ->withPaths([__DIR__ . '/app', __DIR__ . '/routes'])
    ->withSets([NimbusSetList::LARAVEL_TO_NIMBUS])
    ->withImportNames(importShortClasses: false, removeUnusedImports: true);
```

```bash
vendor/bin/rector process --dry-run   # review the diff
vendor/bin/rector process
```

## Rules of `NimbusSetList::LARAVEL_TO_NIMBUS`

| Rule | Laravel | Nimbus |
| --- | --- | --- |
| `RenameClassRector` | `Illuminate\Http\Request`, `JsonResponse`, `Response` | `Pam\Http\Request`, `Pam\Http\Response` |
| | `Illuminate\Foundation\Http\FormRequest` | `Nimbus\Validation\FormRequest` |
| | `JsonResource`, `ResourceCollection` | `Nimbus\Http\JsonResource`, `Nimbus\Http\ResourceCollection` |
| | `Facades\Http`, `Http\Client\{Response,RequestException,ConnectionException,Pool}` | `Nimbus\Http\Client\…` |
| | `Facades\Cache`, `Facades\Hash` | `Nimbus\Cache\Cache`, `Nimbus\Security\Hash` |
| `LaravelRequestCallToNimbusRector` | `$request->query('k')`, `header('k')`, `header()`, `getContent()`, `method()` | `getQuery('k')`, `getHeader('k')`, `headers()`, `body()`, `->method` |
| | `$request->input()`, `all()`, `has()`, `filled()`, `only()`, `bearerToken()`, `json('k')` | `Nimbus\Http\RequestData::…($request, …)` |
| `ResponseHelperToResponseParameterRector` | `response()->json($data, $status)`, `response()->noContent()` | `$response->json($data, $status)`, `$response->status(204)`, adds `Pam\Http\Response $response` to the action |
| `RouteFacadeToRouterRector` | `Route::get(…)`, `Route::prefix()->group(function () {…})` | `$router->get(…)`, `->group(function (RouteRegistrar $router) {…})` |
| | `->middleware('auth')`, `'auth:sanctum'`, `'auth:api'` | `->auth()` |
| `AbortToHttpExceptionRector` | `abort(404, 'msg')`, `abort_if()`, `abort_unless()` | `throw new HttpException(404, ProblemCode::NotFound, 'msg')` |

Left untouched, for review: `response()->json()` with headers or options,
`response()` inside Laravel middleware (`handle($request, Closure $next)`),
array-style `Route::group([...], fn)`, `Route::controller()`, named middleware
strings other than `auth*` (`throttle:…`), `abort()` with a status that has no
`ProblemCode` (for example 402) or a non-literal status.

Laravel prefixes `routes/api.php` with `api`; Nimbus does not, so wrap the
converted file in `$router->prefix('api')->group(...)` or add `api/` to each path.

Measured on `buck-log-intelligence` (29 routes, 11 controllers, 4 middleware):
`rector process --dry-run` changes 17 files of `app/Http` and `routes/api.php`.

## Tests

```bash
pam composer install
vendor/bin/phpunit
```

Each rule has fixtures in `tests/Rector/Fixture/<rule>/*.php.inc` (input, `-----`,
expected output); `Fixture/set` runs the whole set with import names on.
