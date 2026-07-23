<?php

declare(strict_types=1);

require_once __DIR__ . '/../../compat/composer-smoke/vendor/autoload.php';

use Pam\App;
use Pam\Http\Request;
use Pam\Http\Response;

$app = new App();
$app->get('/', static fn (Request $request, Response $response): Response => $response->send('ok'));
$app->listen((int) getenv('PAM_TEST_PORT'), options: [
    'rateLimitPerSecond' => 0,
    'http3' => false,
    'gcCollectCyclesEvery' => 0,
    'gcMemCachesEvery' => 0,
]);
