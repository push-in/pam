<?php

declare(strict_types=1);

use Pam\Http\Request;
use Pam\Http\Response;
use Pam\Http\Server;

Server::create(static function (Request $request, Response $response): Response {
    if ($request->path !== '/echo') {
        return $response->json(['error' => 'Not Found'], 404);
    }

    return $response->json([
        'body' => $request->json(),
        'testHeader' => $request->getHeader('x-pam-test'),
    ]);
})->listen((int) getenv('PAM_TEST_PORT'));
