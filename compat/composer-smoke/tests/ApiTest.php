<?php

declare(strict_types=1);

namespace Pam\Compatibility\Tests;

use Pam\App;
use Pam\Contracts\Http\ApplicationInterface;
use Pam\Contracts\Package\ServiceProviderInterface;
use Pam\Api\Middleware\SecurityHeadersMiddleware;
use Pam\Api\Middleware\RateLimitMiddleware;
use Pam\Api\PackageDiscovery;
use Pam\Api\Router;
use Pam\Api\RoutingResultType;
use Pam\Http\Request;
use Pam\Http\Response;
use Pam\Testing\TestClient;
use PHPUnit\Framework\TestCase;

final class FixtureServiceProvider implements ServiceProviderInterface
{
    public function register(ApplicationInterface $application): void
    {
    }

    public function boot(ApplicationInterface $application): void
    {
    }
}

final class ApiTest extends TestCase
{
    public function testRouterMatchesParametersAndReportsAllowedMethods(): void
    {
        $router = new Router();
        $router->add('GET', '/users/{id}', static fn (): string => 'user');
        $router->add('GET', '/users/me', static fn (): string => 'current-user');

        $match = $router->match('GET', '/users/42');
        self::assertSame(RoutingResultType::Found, $match->type);
        self::assertSame(['id' => '42'], $match->parameters);

        $methodMismatch = $router->match('POST', '/users/42');
        self::assertSame(RoutingResultType::MethodNotAllowed, $methodMismatch->type);
        self::assertSame(['GET'], $methodMismatch->allowedMethods);

        self::assertSame(RoutingResultType::NotFound, $router->match('GET', '/missing')->type);
        self::assertSame('/users/me', $router->match('GET', '/users/me')->route?->path);
    }

    public function testApiPipelineRunsInsidePam(): void
    {
        if (!class_exists(Request::class, false)) {
            self::markTestSkipped('This contract executes inside the Pam Embed SAPI.');
        }

        $app = new App(discoverPackages: false);
        $app->middleware(new SecurityHeadersMiddleware(hsts: false));
        $app->get('/users/{id}', static fn (Request $request, Response $response): Response =>
            $response->json(['id' => $request->route('id')]));

        $response = $app->handle(
            new Request('GET', '/users/42', [], [], ''),
            new Response(),
        )->export();

        self::assertSame(200, $response['status']);
        self::assertSame('{"id":"42"}', $response['body']);
        self::assertSame(['nosniff'], $response['headers']['x-content-type-options']);
    }

    public function testInMemoryClientExercisesTheApiWithoutOpeningAPort(): void
    {
        if (!class_exists(Request::class, false)) {
            self::markTestSkipped('This contract executes inside the Pam Embed SAPI.');
        }

        $app = new App(discoverPackages: false);
        $app->get('/users/{id}', static fn (Request $request, Response $response): Response =>
            $response->json([
                'id' => $request->route('id'),
                'expand' => $request->getQuery('expand'),
            ]));

        $testResponse = (new TestClient($app))
            ->get('/users/42?expand=profile')
            ->assertSuccessful()
            ->assertHeader('content-type', 'application/json; charset=utf-8')
            ->assertJsonPath('id', '42')
            ->assertJson([
                'id' => '42',
                'expand' => 'profile',
            ]);
        self::assertSame(['id' => '42', 'expand' => 'profile'], $testResponse->json());
    }

    public function testComposerProviderDiscoveryBuildsAnAtomicCache(): void
    {
        $root = sys_get_temp_dir() . '/pam-package-discovery-' . bin2hex(random_bytes(8));
        $composerDirectory = $root . '/vendor/composer';
        self::assertTrue(mkdir($composerDirectory, 0755, true));

        try {
            file_put_contents($composerDirectory . '/installed.json', json_encode([
                'packages' => [[
                    'name' => 'acme/pam-fixture',
                    'extra' => [
                        'pam' => [
                            'providers' => [FixtureServiceProvider::class],
                        ],
                    ],
                ]],
            ], JSON_THROW_ON_ERROR));

            self::assertSame([FixtureServiceProvider::class], PackageDiscovery::providers($root));
            self::assertFileExists($root . '/.pam/cache/packages.json');
            self::assertSame([FixtureServiceProvider::class], PackageDiscovery::providers($root));

            file_put_contents($root . '/.pam/cache/packages.json', '<?php echo "unsafe";');
            touch($root . '/.pam/cache/packages.json', time() + 5);
            $this->expectException(\JsonException::class);
            PackageDiscovery::providers($root);
        } finally {
            self::removeDirectory($root);
        }
    }

    public function testRateLimitStateIsBoundedAndHeadersRejectInjection(): void
    {
        if (!class_exists(Request::class, false)) {
            self::markTestSkipped('This contract executes inside the Pam Embed SAPI.');
        }

        $server = $_SERVER;
        try {
            $app = new App(discoverPackages: false);
            $app->middleware(new RateLimitMiddleware(100, maxBuckets: 1));
            $app->get('/ping', static fn (Request $request, Response $response): Response =>
                $response->json(['message' => 'pong']));
            $client = new TestClient($app);

            $_SERVER['REMOTE_ADDR'] = '192.0.2.1';
            self::assertSame(200, $client->get('/ping')->status);
            $_SERVER['REMOTE_ADDR'] = '192.0.2.2';
            self::assertSame(429, $client->get('/ping')->status);

            $this->expectException(\InvalidArgumentException::class);
            (new Response())->header('x-safe', "value\r\nx-injected: true");
        } finally {
            $_SERVER = $server;
        }
    }

    private static function removeDirectory(string $directory): void
    {
        if (!is_dir($directory)) {
            return;
        }
        $iterator = new \RecursiveIteratorIterator(
            new \RecursiveDirectoryIterator($directory, \FilesystemIterator::SKIP_DOTS),
            \RecursiveIteratorIterator::CHILD_FIRST,
        );
        foreach ($iterator as $entry) {
            if (!$entry instanceof \SplFileInfo) {
                continue;
            }
            if ($entry->isDir()) {
                rmdir($entry->getPathname());
            } else {
                unlink($entry->getPathname());
            }
        }
        rmdir($directory);
    }
}
