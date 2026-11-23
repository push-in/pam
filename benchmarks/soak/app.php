<?php

declare(strict_types=1);

// Application for the nightly RSS soak and the PR throughput gate
// (scripts/soak.sh, scripts/perf-gate.sh). It covers the request paths a
// production app exercises: a plain JSON answer, request headers (names the
// load generator varies), bodies, and a request that suspends its Fiber.

require_once __DIR__ . '/../../compat/composer-smoke/vendor/autoload.php';

use Nimbus\App;
use Pam\Http\Request;
use Pam\Http\Response;

$app = new App();
$app->get('/ping', static fn (Request $request, Response $response): Response => $response->json([
    'message' => 'pong',
]));
$app->get('/headers', static fn (Request $request, Response $response): Response => $response->json([
    'server' => count($_SERVER),
    'requestId' => $_SERVER['PAM_REQUEST_ID'] ?? '',
]));
$app->post('/echo', static fn (Request $request, Response $response): Response => $response->json([
    'bytes' => strlen($request->body()),
    'sha1' => sha1($request->body()),
]));
$app->get('/async', static function (Request $request, Response $response): Response {
    \Pam\Async\delay(0.001);

    return $response->json(['suspended' => true]);
});

$app->listen((int) (getenv('PAM_SOAK_PORT') ?: 8080), options: [
    'rateLimitPerSecond' => 0,
    'maxBodyBytes' => 64 * 1024,
]);
