<?php

declare(strict_types=1);

namespace Pam\Api\Tests;

use Pam\App;
use Pam\Api\RouteRegistrar;
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
}

