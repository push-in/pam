<?php

declare(strict_types=1);

use App\Http\Controllers\PingController;
use Pam\Api\Config\ConfigDefinition;
use Pam\Api\Config\Configuration;
use Pam\Api\Config\ConfigType;
use Pam\Api\Middleware\SecurityHeadersMiddleware;
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
$app->middleware(new SecurityHeadersMiddleware());
$app->get('/api/ping', [PingController::class, 'show']);
$app->listen($config->integer('app.port'));
