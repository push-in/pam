<?php

declare(strict_types=1);

namespace Pam\Octane\Tests;

use Laravel\Octane\ApplicationFactory;
use Laravel\Octane\RequestContext;
use Laravel\Octane\Worker;
use Pam\Http\Request as PamRequest;
use Pam\Http\Response as PamResponse;
use Pam\Octane\PamClient;
use PHPUnit\Framework\Attributes\RunInSeparateProcess;
use PHPUnit\Framework\TestCase;

final class OctaneWorkerIntegrationTest extends TestCase
{
    #[RunInSeparateProcess]
    public function testTheRealOctaneWorkerStaysWarmAndIsolatesRequests(): void
    {
        $_ENV['APP_KEY'] = $_SERVER['APP_KEY'] = 'base64:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=';
        $_ENV['APP_ENV'] = $_SERVER['APP_ENV'] = 'testing';
        $basePath = __DIR__.'/Fixtures/laravel';
        $client = new PamClient();
        $worker = new Worker(new ApplicationFactory($basePath), $client);
        $worker->boot();

        try {
            $first = $this->dispatch($worker, $client, '/state');
            $second = $this->dispatch($worker, $client, '/isolation');
            $third = $this->dispatch($worker, $client, '/state');
        } finally {
            $worker->terminate();
        }

        self::assertSame(200, $first['status']);
        self::assertSame(200, $second['status']);
        self::assertSame(200, $third['status']);

        $firstPayload = $this->json($first['body']);
        $secondPayload = $this->json($second['body']);
        $thirdPayload = $this->json($third['body']);

        self::assertSame('state', $firstPayload['path']);
        self::assertFalse($secondPayload['leaked']);
        self::assertSame($firstPayload['boot_id'], $thirdPayload['boot_id']);
    }

    /** @return array{status: int, headers: array<string, list<string>>, body: string, chunks: list<string>} */
    private function dispatch(Worker $worker, PamClient $client, string $path): array
    {
        $_GET = $_POST = $_COOKIE = $_FILES = [];
        $_SERVER = [
            'PHP_SELF' => 'artisan',
            'REQUEST_METHOD' => 'GET',
            'REQUEST_URI' => $path,
            'SERVER_NAME' => 'localhost',
            'SERVER_PORT' => '8000',
            'HTTP_ACCEPT' => 'application/json',
        ];

        $response = new PamResponse();
        $context = new RequestContext([
            'pamRequest' => new PamRequest('GET', $path, [], ['accept' => ['application/json']], ''),
            'pamResponse' => $response,
        ]);

        $worker->handle(...$client->marshalRequest($context));

        return $response->export();
    }

    /** @return array<string, mixed> */
    private function json(string $body): array
    {
        $decoded = json_decode($body, true, 512, JSON_THROW_ON_ERROR);
        self::assertIsArray($decoded);

        return $decoded;
    }
}
