<?php

declare(strict_types=1);

use Pam\App;
use Pam\Api\Middleware\SecurityHeadersMiddleware;
use Pam\Http\Request;
use Pam\Http\Response;

$app = new App();
$app->middleware(new SecurityHeadersMiddleware());

$app->get('/api/ping', static fn (Request $request, Response $response): Response =>
    $response->json([
        'message' => 'pong',
        'requestId' => $_SERVER['PAM_REQUEST_ID'] ?? null,
    ]));

$app->get('/api/users/{id}', static fn (Request $request, Response $response): Response =>
    $response->json([
        'id' => $request->route('id'),
    ]));

$app->listen((int) (getenv('PAM_PORT') ?: 3000));
