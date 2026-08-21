<?php

declare(strict_types=1);

use Pam\App;
use Pam\Api\Middleware\SecurityHeadersMiddleware;
use App\Http\Controllers\PingController;
use Pam\Http\Request;
use Pam\Http\Response;

$app = new App();
$app->middleware(new SecurityHeadersMiddleware());

$app->get('/api/ping', [PingController::class, 'show']);

$app->get('/api/users/{id}', static fn (Request $request, Response $response): Response =>
    $response->json([
        'id' => $request->route('id'),
    ]));

$app->listen((int) (getenv('PAM_PORT') ?: 3000));
