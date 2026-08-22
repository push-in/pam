<?php

declare(strict_types=1);

use Pam\App;
use Pam\Api\Middleware\CorsMiddleware;
use Pam\Api\Middleware\RateLimitMiddleware;
use Pam\Api\Middleware\SecurityHeadersMiddleware;
use Pam\Http\Request;
use Pam\Http\Response;

$app = new App(discoverPackages: false);
$app->middleware(new CorsMiddleware(['https://app.example']));
$app->middleware(new RateLimitMiddleware(requestsPerSecond: 100));
$app->middleware(new SecurityHeadersMiddleware());
$app->get('/users/{id}', static fn (Request $request, Response $response): Response =>
    $response->json(['id' => $request->route('id')]));

return $app;
