<?php

declare(strict_types=1);

use App\Http\Controllers\PingController;
use App\Http\Controllers\ProductController;
use App\Providers\AppServiceProvider;
use Pam\Api\Config\ConfigDefinition;
use Pam\Api\Config\Configuration;
use Pam\Api\Config\ConfigType;
use Pam\Api\Middleware\SecurityHeadersMiddleware;
use Pam\Api\Database\ArcanoSchemaCompiler;
use Pam\Api\Database\ArcanoServiceProvider;
use Pam\Api\Database\DatabaseConfig;
use Pam\App;

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
    new ArcanoSchemaCompiler(__DIR__ . '/src/Models', __DIR__ . '/var/cache/arcano-schema.php', refresh: getenv('PAM_ENV') !== 'production'),
    migrations: __DIR__ . '/database/migrations',
));
$app->provider(new AppServiceProvider());
$app->middleware(new SecurityHeadersMiddleware());
$app->get('/api/ping', [PingController::class, 'show']);
$app->get('/api/products', [ProductController::class, 'index']);
$app->post('/api/products', [ProductController::class, 'store']);
$app->listen($config->integer('app.port'));
