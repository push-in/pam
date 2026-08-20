<?php

declare(strict_types=1);

require dirname(__DIR__) . '/vendor/autoload.php';

use Pam\Api\RouteConstraint;
use Pam\Api\Router;

$router = new Router();
for ($index = 1; $index <= 100; ++$index) {
    $router->add('GET', "/static/{$index}", static fn (): null => null);
}
$dynamic = $router->register('GET', '/users/{id}', static fn (): null => null);
$router->constrain($dynamic, 'id', RouteConstraint::Integer);

$iterations = 100_000;
$startedAt = hrtime(true);
for ($index = 0; $index < $iterations; ++$index) {
    $router->match('GET', $index % 2 === 0 ? '/static/50' : '/users/42');
}
$seconds = (hrtime(true) - $startedAt) / 1_000_000_000;

echo json_encode([
    'iterations' => $iterations,
    'seconds' => $seconds,
    'matchesPerSecond' => (int) round($iterations / $seconds),
], JSON_THROW_ON_ERROR | JSON_PRETTY_PRINT), "\n";
