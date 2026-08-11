<?php

declare(strict_types=1);

use Laravel\Octane\ApplicationFactory;
use Laravel\Octane\RequestContext;
use Laravel\Octane\Worker;
use Pam\Http\Request;
use Pam\Http\Response;
use Pam\Octane\PamClient;

$root = dirname(__DIR__, 2);
require $root.'/packages/octane/tests/bootstrap.php';

$basePath = $root.'/packages/octane/tests/Fixtures/laravel';
$_ENV['APP_ENV'] = $_SERVER['APP_ENV'] = 'production';
$_ENV['APP_DEBUG'] = $_SERVER['APP_DEBUG'] = 'false';
$_ENV['APP_KEY'] = $_SERVER['APP_KEY'] = 'base64:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=';

$iterations = (int) ($argv[1] ?? 10_000);
$client = new PamClient();
$worker = new Worker(new ApplicationFactory($basePath), $client);
$worker->boot();
$started = hrtime(true);

try {
    for ($iteration = 0; $iteration < $iterations; ++$iteration) {
        $_GET = $_POST = $_COOKIE = $_FILES = [];
        $_SERVER = [
            'REQUEST_METHOD' => 'GET',
            'REQUEST_URI' => '/api/ping',
            'SERVER_NAME' => 'localhost',
            'SERVER_PORT' => '8000',
            'HTTP_ACCEPT' => 'application/json',
        ];
        $context = new RequestContext([
            'pamRequest' => new Request('GET', '/api/ping', [], ['accept' => ['application/json']], ''),
            'pamResponse' => new Response(),
        ]);
        ob_start();
        try {
            $worker->handle(...$client->marshalRequest($context));
        } finally {
            ob_end_clean();
            header_remove();
            http_response_code(200);
        }
    }
} finally {
    $worker->terminate();
}

$seconds = (hrtime(true) - $started) / 1_000_000_000;
echo json_encode([
    'iterations' => $iterations,
    'seconds' => $seconds,
    'requests_per_second' => $iterations / $seconds,
    'microseconds_per_request' => ($seconds * 1_000_000) / $iterations,
], JSON_PRETTY_PRINT | JSON_THROW_ON_ERROR), PHP_EOL;
