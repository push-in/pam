<?php

declare(strict_types=1);

namespace Pam\Api\Tests;

use Pam\App;
use Pam\Api\OpenApi\ClientLanguage;
use Pam\Api\Testing\TestClient;
use Pam\Http\Request;
use Pam\Http\Response;
use PHPUnit\Framework\TestCase;

final class TestingAndClientGenerationTest extends TestCase
{
    public function testInMemoryClientOffersFluentAssertions(): void
    {
        $app = new App(discoverPackages: false);
        $app->post('/login', static fn (Request $request, Response $response): Response =>
            $response->header('x-auth-state', 'created')->json(['data' => ['status' => 1]], 201));

        $result = (new TestClient($app))
            ->postJson('/login', ['email' => 'dev@pam.dev'])
            ->assertStatus(201)
            ->assertSuccessful()
            ->assertHeader('x-auth-state', 'created')
            ->assertJson(['data' => ['status' => 1]])
            ->assertJsonPath('data.status', 1);

        self::assertSame(201, $result->status());
    }

    public function testOpenApiGeneratesThreeTypedClientSurfaces(): void
    {
        $app = new App(discoverPackages: false);
        $app->get('/users', static fn (Request $request, Response $response): Response => $response->json([]))
            ->name('users.index');
        $openApi = $app->openApi();

        self::assertStringContainsString('users_index', $openApi->client(ClientLanguage::TypeScript));
        self::assertStringContainsString('users_index', $openApi->client(ClientLanguage::Kotlin));
        self::assertStringContainsString('users_index', $openApi->client(ClientLanguage::Swift));

        $app->get('/users/{id}', static fn (Request $request, Response $response): Response => $response->json([]))
            ->name('users.show');
        self::assertStringContainsString(
            'encodeURIComponent(String(id))',
            $app->openApi()->client(ClientLanguage::TypeScript),
        );
    }
}
