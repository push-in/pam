<?php

declare(strict_types=1);

use App\Providers\AppServiceProvider;
use Nimbus\App;
use Nimbus\Database\ArcanoSchemaCompiler;
use Nimbus\Database\ArcanoServiceProvider;
use Nimbus\Database\DatabaseConfig;
use Nimbus\Middleware\SecurityHeadersMiddleware;

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
$app->routes($root . '/routes.php');

return $app;
