<?php

declare(strict_types=1);

use Pam\App;
use App\Http\Controllers\PingController;
use Pam\Http\Request;
use Pam\Http\Response;
use Pam\Testing\TestClient;
use PHPUnit\Framework\TestCase;

final class ApplicationTest extends TestCase
{
    public function testPingEndpoint(): void
    {
        $app = new App(discoverPackages: false);
        $app->get('/api/ping', [PingController::class, 'show']);

        (new TestClient($app))
            ->get('/api/ping')
            ->assertSuccessful()
            ->assertJsonPath('data.status', 1)
            ->assertJsonPath('data.message', 'pong');
        self::addToAssertionCount(1);
    }
}
