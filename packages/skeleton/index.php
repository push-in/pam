<?php

declare(strict_types=1);

use Nimbus\Config\ConfigDefinition;
use Nimbus\Config\Configuration;
use Nimbus\Config\ConfigType;

$config = Configuration::fromEnvironment([
    new ConfigDefinition(
        key: 'app.port',
        environment: 'PAM_PORT',
        type: ConfigType::Integer,
        required: false,
        default: 3000,
    ),
    new ConfigDefinition(
        key: 'app.host',
        environment: 'PAM_HOST',
        type: ConfigType::String,
        required: false,
        // Containers set PAM_HOST=0.0.0.0; a laptop keeps the API on loopback.
        default: '127.0.0.1',
    ),
]);
/** @var \Nimbus\App $app */
$app = require __DIR__ . '/bootstrap/app.php';
$app->listen($config->integer('app.port'), $config->string('app.host'));
