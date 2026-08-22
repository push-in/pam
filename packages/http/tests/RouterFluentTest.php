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

    public function testOptionsIsAutomaticAndAdvertisesHeadForGetRoutes(): void
    {
        $app = new App(discoverPackages: false);
        $app->get('/users', static fn (Request $request, Response $response): Response => $response->send('users'));
        $app->post('/users', static fn (Request $request, Response $response): Response => $response->send('created'));

        $options = $app->handle($this->request('OPTIONS', '/users'), new Response())->export();
        $invalid = $app->handle($this->request('PATCH', '/users'), new Response())->export();

        self::assertSame(204, $options['status']);
        self::assertSame('', $options['body']);
        self::assertSame(['GET, HEAD, OPTIONS, POST'], $options['headers']['allow']);
        self::assertSame(['GET, HEAD, OPTIONS, POST'], $invalid['headers']['allow']);
    }

    public function testExplicitOptionsRouteOverridesAutomaticResponse(): void
    {
        $app = new App(discoverPackages: false);
        $app->get('/resource', static fn (Request $request, Response $response): Response => $response->send('resource'));
        $app->options('/resource', static fn (Request $request, Response $response): Response =>
            $response->header('x-options', 'custom')->status(200));

        $result = $app->handle($this->request('OPTIONS', '/resource'), new Response())->export();

        self::assertSame(200, $result['status']);
        self::assertSame(['custom'], $result['headers']['x-options']);
    }

    public function testHeadPreservesGetMetadataAndSuppressesItsBody(): void
    {
        $app = new App(discoverPackages: false);
        $app->get('/report', static fn (Request $request, Response $response): Response =>
            $response->status(202)->header('x-report', 'ready')->send('must-not-leave-worker'));

        $result = $app->handle($this->request('HEAD', '/report'), new Response())->export();

        self::assertSame(202, $result['status']);
        self::assertSame(['ready'], $result['headers']['x-report']);
        self::assertSame('', $result['body']);
    }

    private function request(string $method, string $path): Request
    {
        return new Request($method, $path, [], [], '');
    }
}
