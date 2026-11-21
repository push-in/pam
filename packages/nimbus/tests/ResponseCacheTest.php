<?php

declare(strict_types=1);

namespace Nimbus\Tests;

use Nimbus\App;
use Nimbus\Cache\MemoryResponseCacheStore;
use Nimbus\Middleware\ResponseCacheMiddleware;
use Pam\Http\Request;
use Pam\Http\Response;
use PHPUnit\Framework\TestCase;

final class ResponseCacheTest extends TestCase
{
    public function testSuccessfulGetResponsesAreCached(): void
    {
        $app = new App(discoverPackages: false);
        $calls = 0;
        $app->get('/products', static function (Request $request, Response $response) use (&$calls): Response {
            return $response->json(['generation' => ++$calls]);
        })->middleware(new ResponseCacheMiddleware(new MemoryResponseCacheStore()));

        $first = $app->handle(new Request('GET', '/products', [], [], ''), new Response())->export();
        $second = $app->handle(new Request('GET', '/products', [], [], ''), new Response())->export();

        self::assertSame(1, $calls);
        self::assertSame(['MISS'], $first['headers']['x-cache']);
        self::assertSame(['HIT'], $second['headers']['x-cache']);
        self::assertSame($first['body'], $second['body']);
    }

    public function testResponsesAreCachedPerCredential(): void
    {
        $app = new App(discoverPackages: false);
        $app->get('/me', static fn (Request $request, Response $response): Response => $response->json([
            'user' => $request->getHeader('authorization') ?? $request->getHeader('cookie') ?? 'guest',
        ]))->middleware(new ResponseCacheMiddleware(new MemoryResponseCacheStore()));

        $get = static fn (array $headers): string => $app->handle(
            new Request('GET', '/me', [], $headers, ''),
            new Response(),
        )->export()['body'];

        self::assertStringContainsString('Bearer alice', $get(['authorization' => ['Bearer alice']]));
        self::assertStringContainsString('Bearer bob', $get(['authorization' => ['Bearer bob']]));
        self::assertStringContainsString('session=bob', $get(['cookie' => ['session=bob']]));
        self::assertStringContainsString('guest', $get([]));
        self::assertStringContainsString('Bearer alice', $get(['authorization' => ['Bearer alice']]));
    }

    public function testPrivateResponsesAndCookiesAreNeverStored(): void
    {
        $app = new App(discoverPackages: false);
        $calls = 0;
        $cache = new ResponseCacheMiddleware(new MemoryResponseCacheStore());
        $app->get('/login-page', static function (Request $request, Response $response) use (&$calls): Response {
            return $response->header('set-cookie', 'session=' . ++$calls)->json(['call' => $calls]);
        })->middleware($cache);
        $app->get('/private', static function (Request $request, Response $response) use (&$calls): Response {
            return $response->header('cache-control', 'private, max-age=60')->json(['call' => ++$calls]);
        })->middleware($cache);

        foreach (['/login-page', '/private'] as $path) {
            $first = $app->handle(new Request('GET', $path, [], [], ''), new Response())->export();
            $second = $app->handle(new Request('GET', $path, [], [], ''), new Response())->export();
            self::assertNotSame($first['body'], $second['body'], $path);
            self::assertSame(['MISS'], $second['headers']['x-cache'], $path);
        }
    }
}
