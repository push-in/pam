<?php

declare(strict_types=1);

namespace Pam\Api\Tests;

use Pam\App;
use Pam\Api\RouteConstraint;
use Pam\Contracts\Http\RequestHandlerInterface;
use Pam\Http\Request;
use Pam\Http\Response;
use PHPUnit\Framework\TestCase;

final class RouterFluentTest extends TestCase
{
    public function testLegacyVerbChainingRemainsSupported(): void
    {
        $app = new App(discoverPackages: false);
        $app->get('/one', static fn (Request $request, Response $response): Response => $response->send('one'))
            ->post('/two', static fn (Request $request, Response $response): Response => $response->send('two'));

        self::assertSame(200, $app->handle($this->request('POST', '/two'), new Response())->export()['status']);
    }

    public function testAConstraintPreventsAnInvalidDynamicMatch(): void
    {
        $app = new App(discoverPackages: false);
        $app->get('/users/{id}', static fn (Request $request, Response $response): Response =>
            $response->json(['id' => $request->route('id')]))
            ->where('id', RouteConstraint::Integer)
            ->name('users.show');

        $valid = $app->handle($this->request('GET', '/users/42'), new Response())->export();
        $invalid = $app->handle($this->request('GET', '/users/not-an-id'), new Response())->export();

        self::assertSame(200, $valid['status']);
        self::assertSame(404, $invalid['status']);
    }

    public function testRouteMiddlewareOnlyWrapsItsOwnRoute(): void
    {
        $app = new App(discoverPackages: false);
        $middleware = static function (
            Request $request,
            Response $response,
            RequestHandlerInterface $next,
        ): Response {
            return $next->handle($request, $response)->header('x-route', 'wrapped');
        };
        $app->get('/wrapped', static fn (Request $request, Response $response): Response => $response->send('ok'))
            ->middleware($middleware);
        $app->get('/plain', static fn (Request $request, Response $response): Response => $response->send('ok'));

        $wrapped = $app->handle($this->request('GET', '/wrapped'), new Response())->export();
        $plain = $app->handle($this->request('GET', '/plain'), new Response())->export();

        self::assertSame(['wrapped'], $wrapped['headers']['x-route']);
        self::assertArrayNotHasKey('x-route', $plain['headers']);
    }

    private function request(string $method, string $path): Request
    {
        return new Request($method, $path, [], [], '');
    }
}
