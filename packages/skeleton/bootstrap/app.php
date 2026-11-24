<?php

declare(strict_types=1);

use App\Providers\AppServiceProvider;
use Nimbus\App;
use Nimbus\Database\ArcanoManager;
use Nimbus\Database\ArcanoSchemaCompiler;
use Nimbus\Database\ArcanoServiceProvider;
use Nimbus\Database\DatabaseConfig;
use Nimbus\Database\DatabaseHealthCheck;
use Nimbus\Health\HealthEndpoints;
use Nimbus\Health\HealthRegistry;
use Nimbus\Middleware\SecurityHeadersMiddleware;
use Nimbus\Sentry\SentryReporter;
use Nimbus\Telemetry\Telemetry;

// Builds the application without listening: index.php serves it and bin/nimbus tinker opens it.
$root = dirname(__DIR__);

$app = new App();
$app->provider(new ArcanoServiceProvider(
    DatabaseConfig::fromEnvironment(),
    // Production builds the schema cache on deploy (bin/migrate cache) and never rescans.
    new ArcanoSchemaCompiler($root . '/app/Models', $root . '/var/cache/arcano-schema.php', refresh: getenv('PAM_ENV') !== 'production'),
    migrations: $root . '/database/migrations',
));
$app->provider(new AppServiceProvider());
$app->middleware(new SecurityHeadersMiddleware());

// GET /health (liveness) and GET /ready (readiness: database reachable, worker not draining).
$arcano = $app->container()->get(ArcanoManager::class);
assert($arcano instanceof ArcanoManager);
(new HealthEndpoints((new HealthRegistry())->add('database', new DatabaseHealthCheck($arcano))))->register($app);

// Off unless configured: NIMBUS_METRICS=1, an OpenTelemetry SDK (OTEL_*), SENTRY_DSN.
Telemetry::fromEnvironment()->install($app);
SentryReporter::fromEnvironment($app->container())?->install($app);

$app->routes($root . '/routes.php');

return $app;
