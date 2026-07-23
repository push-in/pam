<?php

declare(strict_types=1);

use Pam\App;
use Pam\Http\Request;
use Pam\Http\Response;
use Pam\Testing\TestClient;
use PHPUnit\Framework\TestCase;

final class ApplicationTest extends TestCase
{
    public function testPingEndpoint(): void
    {
        $app = new App(discoverPackages: false);
        $app->get('/api/ping', static fn (Request $request, Response $response): Response =>
            $response->json(['message' => 'pong']));

        (new TestClient($app))
            ->get('/api/ping')
            ->assertSuccessful()
            ->assertJson(['message' => 'pong']);
        self::addToAssertionCount(1);
    }
}
