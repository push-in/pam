<?php

declare(strict_types=1);

use Nimbus\App;
use App\Providers\AppServiceProvider;
use Nimbus\Database\ArcanoMigrationManager;
use Nimbus\Database\ArcanoSchemaCompiler;
use Nimbus\Database\ArcanoServiceProvider;
use Nimbus\Database\DatabaseConfig;
use Nimbus\Testing\TestClient;
use PHPUnit\Framework\TestCase;

final class ApplicationTest extends TestCase
{
    private string $database;

    protected function setUp(): void
    {
        $this->database = sys_get_temp_dir() . '/nimbus-skeleton-' . bin2hex(random_bytes(8)) . '.sqlite';
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
        $app->routes(dirname(__DIR__) . '/routes.php');

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
        $app->provider(new ArcanoServiceProvider(
            new DatabaseConfig('default', [
                'default' => ['driver' => 'sqlite', 'database' => $this->database, 'prefix' => ''],
            ]),
            new ArcanoSchemaCompiler(dirname(__DIR__) . '/app/Models'),
            migrations: dirname(__DIR__) . '/database/migrations',
        ));
        $app->provider(new AppServiceProvider());
        $app->routes(dirname(__DIR__) . '/routes.php');
        $app->boot();
        $migrations = $app->container()->get(ArcanoMigrationManager::class);
        if (!$migrations instanceof ArcanoMigrationManager) {
            throw new \LogicException('Migration manager binding is invalid.');
        }
        $migrations->migrate();

        return $app;
    }
}
