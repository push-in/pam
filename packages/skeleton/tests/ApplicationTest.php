<?php

declare(strict_types=1);

use Pam\App;
use App\Http\Controllers\PingController;
use App\Http\Controllers\ProductController;
use App\Providers\AppServiceProvider;
use Pam\Http\Request;
use Pam\Http\Response;
use Pam\Api\Database\DatabaseConfig;
use Pam\Api\Database\EloquentServiceProvider;
use Pam\Api\Testing\TestClient;
use PHPUnit\Framework\TestCase;

final class ApplicationTest extends TestCase
{
    private string $database;

    protected function setUp(): void
    {
        $this->database = sys_get_temp_dir() . '/pam-skeleton-' . bin2hex(random_bytes(8)) . '.sqlite';
        touch($this->database);
    }

    protected function tearDown(): void
    {
        if (is_file($this->database)) {
            unlink($this->database);
        }
    }

    public function testPingEndpoint(): void
    {
        $app = new App(discoverPackages: false);
        $app->get('/api/ping', [PingController::class, 'show']);

        (new TestClient($app))
            ->get('/api/ping', ['x-request-id' => 'starter-test'])
            ->assertStatus(200)
            ->assertJsonPath('data.status', 1)
            ->assertJsonPath('data.message', 'pong')
            ->assertJsonPath('data.requestId', 'starter-test');
        self::addToAssertionCount(4);
    }

    public function testProductFlowUsesFormRequestServiceRepositoryAndResource(): void
    {
        $client = new TestClient($this->application());

        $client->postJson('/api/products', [
            'name' => 'Mechanical keyboard',
            'priceInCents' => 34990,
        ])
            ->assertStatus(201)
            ->assertJsonPath('data.status', 1)
            ->assertJsonPath('data.priceInCents', 34990);

        $client->get('/api/products')
            ->assertStatus(200)
            ->assertJsonPath('data.0.name', 'Mechanical keyboard');

        $client->postJson('/api/products', ['name' => 10])
            ->assertStatus(422)
            ->assertJsonPath('code', 1);
        self::addToAssertionCount(7);
    }

    private function application(): App
    {
        $app = new App(discoverPackages: false);
        $app->provider(new EloquentServiceProvider(new DatabaseConfig('default', [
            'default' => ['driver' => 'sqlite', 'database' => $this->database, 'prefix' => ''],
        ])));
        $app->provider(new AppServiceProvider(dirname(__DIR__) . '/database/migrations'));
        $app->get('/api/products', [ProductController::class, 'index']);
        $app->post('/api/products', [ProductController::class, 'store']);

        return $app;
    }
}
