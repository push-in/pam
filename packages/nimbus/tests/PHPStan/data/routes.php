<?php

declare(strict_types=1);

namespace Nimbus\Tests\PHPStan\data;

use Nimbus\App;
use Pam\Http\Response;

function routes(App $app): void
{
    $app->get('/orders/{id}', static fn (string $id, Response $response): Response => $response->json(['id' => $id]));
    $app->get('/orders/{order}/items', static fn (int $id): int => $id);
    $app->post('/teams/{team}', static function (string $team, ?int $page, int $limit = 10): void {
    });
    $app->get('/pages/{number}', static fn (int $number): int => $number);
}
