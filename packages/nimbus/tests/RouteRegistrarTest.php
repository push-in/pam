<?php

declare(strict_types=1);

namespace Nimbus\Tests;

use Nimbus\App;
use Nimbus\Auth\Authenticator;
use Nimbus\Auth\Principal;
use Nimbus\RouteRegistrar;
use Pam\Contracts\Http\RequestHandlerInterface;
use Pam\Http\Request;
use Pam\Http\Response;
use PHPUnit\Framework\TestCase;

final class RouteRegistrarTest extends TestCase
{
    public function testPrefixAndGroupMiddlewareCompose(): void
    {
        $app = new App(discoverPackages: false);
        $middleware = static function (
            Request $request,
            Response $response,
            RequestHandlerInterface $next,
        ): Response {
            return $next->handle($request, $response)->header('x-api', 'v1');
        };

        $app->prefix('/api')
            ->middleware($middleware)
            ->prefix('/v1')
            ->group(static function (RouteRegistrar $routes): void {
                $routes->get('/ping', static fn (Request $request, Response $response): Response =>
                    $response->json(['message' => 'pong']));
            });

        $response = $app->handle(
            new Request('GET', '/api/v1/ping', [], [], ''),
            new Response(),
        )->export();

        self::assertSame(200, $response['status']);
        self::assertSame(['v1'], $response['headers']['x-api']);
    }

    public function testControllerClosureRoutesNameOnlyTheMethod(): void
    {
        $app = new App(discoverPackages: false);
        $app->controller(RegistrarProductController::class, static function (RouteRegistrar $router): void {
            $router->get('products', 'index');
            $router->get('products/{product}', 'show');
        });

        self::assertSame(['action' => 'index'], self::json($app, 'GET', '/products'));
        self::assertSame(['action' => 'show', 'product' => '7'], self::json($app, 'GET', '/products/7'));
    }

    public function testResourceRegistersTheCrudAndItsClosureExtras(): void
    {
        $app = new App(discoverPackages: false);
        $app->resource('products', RegistrarProductController::class, static function (RouteRegistrar $products): void {
            $products->post('{product}/publish', 'publish');
        });

        self::assertSame(['action' => 'index'], self::json($app, 'GET', '/products'));
        self::assertSame(['action' => 'store'], self::json($app, 'POST', '/products'));
        self::assertSame(['action' => 'destroy', 'product' => '3'], self::json($app, 'DELETE', '/products/3'));
        self::assertSame(['action' => 'publish', 'product' => '3'], self::json($app, 'POST', '/products/3/publish'));
    }

    public function testRoutesFileReceivesTheRouter(): void
    {
        $file = tempnam(sys_get_temp_dir(), 'nimbus-routes-');
        self::assertIsString($file);
        file_put_contents($file, <<<'PHP'
            <?php
            $router->get('ping', fn ($request, $response) => $response->json(['pong' => true]));
            $router->controller(\Nimbus\Tests\RegistrarProductController::class, function ($router) {
                $router->get('products', 'index');
            });
            PHP);
        try {
            $app = (new App(discoverPackages: false))->routes($file);
            self::assertSame(['pong' => true], self::json($app, 'GET', '/ping'));
            self::assertSame(['action' => 'index'], self::json($app, 'GET', '/products'));
        } finally {
            unlink($file);
        }
    }

    public function testAuthAndCanGuardRoutesAndGroups(): void
    {
        $app = new App(discoverPackages: false);
        $app->container()->instance(Authenticator::class, new HeaderAuthenticator());
        $ok = static fn (Request $request, Response $response): Response => $response->json(['ok' => true]);
        $app->get('me', $ok)->auth();
        $app->post('posts/{post}/publish', $ok)->can('publish');
        $admin = $app->prefix('admin')->can('admin');
        $admin->get('stats', $ok);

        $status = static fn (string $method, string $path, string $user = ''): int => $app->handle(
            new Request($method, $path, [], $user === '' ? [] : ['x-user' => [$user]], ''),
            new Response(),
        )->export()['status'];

        self::assertSame(401, $status('GET', '/me'));
        self::assertSame(200, $status('GET', '/me', 'ana'));
        self::assertSame(401, $status('POST', '/posts/1/publish'));
        self::assertSame(403, $status('POST', '/posts/1/publish', 'ana'));
        self::assertSame(200, $status('POST', '/posts/1/publish', 'ana:publish'));
        self::assertSame(403, $status('GET', '/admin/stats', 'ana:publish'));
        self::assertSame(200, $status('GET', '/admin/stats', 'root:admin'));
    }

    /** @return array<array-key, mixed> */
    private static function json(App $app, string $method, string $path): array
    {
        $response = $app->handle(new Request($method, $path, [], [], ''), new Response())->export();
        self::assertSame(200, $response['status'], "{$method} {$path}");
        $body = json_decode((string) $response['body'], true, 512, JSON_THROW_ON_ERROR);
        self::assertIsArray($body);
        return $body;
    }
}

final class RegistrarProductController
{
    public function index(Response $response): Response
    {
        return $response->json(['action' => 'index']);
    }

    public function store(Response $response): Response
    {
        return $response->json(['action' => 'store']);
    }

    public function show(Response $response, string $product): Response
    {
        return $response->json(['action' => 'show', 'product' => $product]);
    }

    public function update(Response $response, string $product): Response
    {
        return $response->json(['action' => 'update', 'product' => $product]);
    }

    public function destroy(Response $response, string $product): Response
    {
        return $response->json(['action' => 'destroy', 'product' => $product]);
    }

    public function publish(Response $response, string $product): Response
    {
        return $response->json(['action' => 'publish', 'product' => $product]);
    }
}

final class HeaderAuthenticator implements Authenticator
{
    public function authenticate(Request $request): ?Principal
    {
        $header = $request->getHeader('x-user');
        if ($header === null) {
            return null;
        }
        $parts = explode(':', $header);
        return new HeaderPrincipal($parts[0], array_slice($parts, 1));
    }
}

final readonly class HeaderPrincipal implements Principal
{
    /** @param list<string> $abilities */
    public function __construct(private string $id, private array $abilities)
    {
    }

    public function identifier(): string
    {
        return $this->id;
    }

    public function can(string $ability): bool
    {
        return in_array($ability, $this->abilities, true);
    }
}
