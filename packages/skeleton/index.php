<?php

declare(strict_types=1);

use App\Http\Controllers\PingController;
use App\Http\Controllers\ProductController;
use App\Providers\AppServiceProvider;
use Pam\Api\Config\ConfigDefinition;
use Pam\Api\Config\Configuration;
use Pam\Api\Config\ConfigType;
use Pam\Api\Middleware\SecurityHeadersMiddleware;
use Pam\Api\Database\DatabaseConfig;
use Pam\Api\Database\EloquentServiceProvider;
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
$app->provider(new EloquentServiceProvider(DatabaseConfig::fromEnvironment()));
$app->provider(new AppServiceProvider(__DIR__ . '/database/migrations'));
$app->middleware(new SecurityHeadersMiddleware());
$app->get('/api/ping', [PingController::class, 'show']);
$app->get('/api/products', [ProductController::class, 'index']);
$app->post('/api/products', [ProductController::class, 'store']);
$app->listen($config->integer('app.port'));
