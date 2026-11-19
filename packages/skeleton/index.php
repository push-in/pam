<?php

declare(strict_types=1);

use App\Providers\AppServiceProvider;
use Nimbus\Config\ConfigDefinition;
use Nimbus\Config\Configuration;
use Nimbus\Config\ConfigType;
use Nimbus\Middleware\SecurityHeadersMiddleware;
use Nimbus\Database\ArcanoSchemaCompiler;
use Nimbus\Database\ArcanoServiceProvider;
use Nimbus\Database\DatabaseConfig;
use Nimbus\App;

$config = Configuration::fromEnvironment([
    new ConfigDefinition(
        key: 'app.port',
        environment: 'PAM_PORT',
        type: ConfigType::Integer,
        required: false,
        default: 3000,
    ),
]);
$app = new App();
$app->provider(new ArcanoServiceProvider(
    DatabaseConfig::fromEnvironment(),
    // Production builds the schema cache on deploy (bin/migrate cache) and never rescans.
    new ArcanoSchemaCompiler(__DIR__ . '/app/Models', __DIR__ . '/var/cache/arcano-schema.php', refresh: getenv('PAM_ENV') !== 'production'),
    migrations: __DIR__ . '/database/migrations',
));
$app->provider(new AppServiceProvider());
$app->middleware(new SecurityHeadersMiddleware());
$app->routes(__DIR__ . '/routes.php');
$app->listen($config->integer('app.port'));
