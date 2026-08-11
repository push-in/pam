<?php

declare(strict_types=1);

use Laravel\Octane\ApplicationFactory;
use Laravel\Octane\ApplicationGateway;
use Laravel\Octane\Contracts\Client;
use Laravel\Octane\CurrentApplication;
use Laravel\Octane\OctaneResponse;
use Laravel\Octane\RequestContext;
use Pam\Http\Request;
use Pam\Http\Response;
use Pam\Octane\PamClient;

$root = dirname(__DIR__, 2);
require $root.'/packages/octane/tests/bootstrap.php';

$basePath = $root.'/packages/octane/tests/Fixtures/laravel';
$_ENV['APP_ENV'] = $_SERVER['APP_ENV'] = 'production';
$_ENV['APP_DEBUG'] = $_SERVER['APP_DEBUG'] = 'false';
$_ENV['APP_KEY'] = $_SERVER['APP_KEY'] = 'base64:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=';
$iterations = (int) ($argv[1] ?? 5_000);
$client = new PamClient();
$factory = new ApplicationFactory($basePath);
$app = $factory->createApplication([Client::class => $client]);
$totals = ['request' => 0, 'clone' => 0, 'kernel' => 0, 'respond' => 0, 'terminate' => 0, 'flush' => 0];

for ($iteration = 0; $iteration < $iterations; ++$iteration) {
    $_GET = $_POST = $_COOKIE = $_FILES = [];
    $_SERVER = [
        'REQUEST_METHOD' => 'GET',
        'REQUEST_URI' => '/api/ping',
        'SERVER_NAME' => 'localhost',
        'SERVER_PORT' => '8000',
        'HTTP_ACCEPT' => 'application/json',
    ];
    $mark = hrtime(true);
    $context = new RequestContext([
        'pamRequest' => new Request('GET', '/api/ping', [], ['accept' => ['application/json']], ''),
        'pamResponse' => new Response(),
    ]);
    [$request] = $client->marshalRequest($context);
    $totals['request'] += hrtime(true) - $mark;

    $mark = hrtime(true);
    CurrentApplication::set($sandbox = clone $app);
    $gateway = new ApplicationGateway($app, $sandbox);
    $totals['clone'] += hrtime(true) - $mark;

    $mark = hrtime(true);
    $response = $gateway->handle($request);
    $totals['kernel'] += hrtime(true) - $mark;

    $mark = hrtime(true);
    ob_start();
    try {
        $client->respond($context, new OctaneResponse($response, ''));
    } finally {
        ob_end_clean();
        header_remove();
        http_response_code(200);
    }
    $totals['respond'] += hrtime(true) - $mark;

    $mark = hrtime(true);
    $gateway->terminate($request, $response);
    $totals['terminate'] += hrtime(true) - $mark;

    $mark = hrtime(true);
    $sandbox->flush();
    $app->make('view.engine.resolver')->forget('blade');
    $app->make('view.engine.resolver')->forget('php');
    CurrentApplication::set($app);
    $totals['flush'] += hrtime(true) - $mark;
}

echo json_encode(array_map(
    static fn (int $nanoseconds): float => $nanoseconds / $iterations / 1_000,
    $totals,
), JSON_PRETTY_PRINT | JSON_THROW_ON_ERROR), PHP_EOL;
