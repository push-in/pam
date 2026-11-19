<?php

declare(strict_types=1);

use App\Models\Product;
use App\Providers\AppServiceProvider;
use Database\Factories\ProductFactory;
use Database\Seeders\DatabaseSeeder;
use Nimbus\App;
use Nimbus\Database\ArcanoSchemaCompiler;
use Nimbus\Database\ArcanoServiceProvider;
use Nimbus\Database\DatabaseConfig;
use Nimbus\Testing\RefreshDatabase;
use Nimbus\Testing\TestCase;

final class ApplicationTest extends TestCase
{
    use RefreshDatabase;

    protected function createApplication(): App
    {
        $app = new App(discoverPackages: false);
        $app->provider(new ArcanoServiceProvider(
            new DatabaseConfig('default', [
                'default' => ['driver' => 'sqlite', 'database' => ':memory:', 'prefix' => ''],
            ]),
            new ArcanoSchemaCompiler(dirname(__DIR__) . '/app/Models'),
            migrations: dirname(__DIR__) . '/database/migrations',
        ));
        $app->provider(new AppServiceProvider());
        $app->routes(dirname(__DIR__) . '/routes.php');

        return $app;
    }

    public function testPingEndpoint(): void
    {
        $this->withHeaders(['x-request-id' => 'starter-test'])
            ->getJson('/api/ping')
            ->assertOk()
            ->assertJsonPath('data.status', 1)
            ->assertJsonPath('data.message', 'pong')
            ->assertJsonPath('data.requestId', 'starter-test');
    }

    public function testProductFlowUsesFormRequestServiceRepositoryAndResource(): void
    {
        $this->postJson('/api/products', [
            'name' => 'Mechanical keyboard',
            'priceInCents' => 34990,
        ])
            ->assertCreated()
            ->assertJsonPath('data.status', 1)
            ->assertJsonPath('data.priceInCents', 34990);

        $this->getJson('/api/products')
            ->assertOk()
            ->assertJsonPath('data.0.name', 'Mechanical keyboard');
        $this->assertDatabaseHas('products', ['name' => 'Mechanical keyboard', 'price_in_cents' => 34990]);

        $this->postJson('/api/products', ['name' => 10])
            ->assertUnprocessableEntity()
            ->assertJsonValidationErrors(['name', 'priceInCents']);
        $this->assertDatabaseCount(Product::class, 1);
    }

    public function testFactoriesAndSeeders(): void
    {
        ProductFactory::new()->count(2)->sequence(['name' => 'Keyboard'], ['name' => 'Mouse'])->create();

        $this->getJson('/api/products')
            ->assertOk()
            ->assertJsonCount(2, 'data')
            ->assertJsonPath('data.1.name', 'Mouse');

        $this->seed(DatabaseSeeder::class);
        $this->assertDatabaseCount('products', 14);
        $this->assertDatabaseHas('products', ['status' => 2]);
    }
}
