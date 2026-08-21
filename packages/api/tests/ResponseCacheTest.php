<?php

declare(strict_types=1);

namespace Pam\Api\Tests;

use Pam\App;
use Pam\Api\Cache\MemoryResponseCacheStore;
use Pam\Api\Middleware\ResponseCacheMiddleware;
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
}

