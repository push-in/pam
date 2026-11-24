<?php

declare(strict_types=1);

namespace Nimbus\Tests\Http;

use Nimbus\Http\RequestData;
use Pam\Http\Request;
use PHPUnit\Framework\TestCase;

final class RequestDataTest extends TestCase
{
    public function testBodyFieldsWinOverQueryParameters(): void
    {
        $request = $this->json(['name' => 'Keyboard', 'items' => [['sku' => 'K-1']], 'empty' => ' ', 'none' => null], ['name' => 'query', 'page' => '2']);

        self::assertSame(['name' => 'Keyboard', 'page' => '2', 'items' => [['sku' => 'K-1']], 'empty' => ' ', 'none' => null], RequestData::all($request));
        self::assertSame('Keyboard', RequestData::input($request, 'name'));
        self::assertSame('2', RequestData::input($request, 'page'));
        self::assertSame('K-1', RequestData::input($request, 'items.0.sku'));
        self::assertSame('fallback', RequestData::input($request, 'missing', 'fallback'));
        self::assertNull(RequestData::input($request, 'none', 'fallback'));
        self::assertSame(RequestData::all($request), RequestData::input($request));
    }

    public function testPresenceAndSelection(): void
    {
        $request = $this->json(['name' => 'Keyboard', 'items' => [['sku' => 'K-1']], 'empty' => ' ', 'none' => null], ['page' => '2']);

        self::assertTrue(RequestData::has($request, 'none'));
        self::assertTrue(RequestData::has($request, ['name', 'items.0.sku']));
        self::assertFalse(RequestData::has($request, ['name', 'missing']));
        self::assertTrue(RequestData::filled($request, 'name'));
        self::assertFalse(RequestData::filled($request, 'empty'));
        self::assertFalse(RequestData::filled($request, 'none'));
        self::assertSame(['name' => 'Keyboard', 'page' => '2'], RequestData::only($request, 'name', 'page', 'missing'));
        self::assertSame(['name' => 'Keyboard'], RequestData::only($request, ['name']));
    }

    public function testUrlencodedBody(): void
    {
        $request = new Request('POST', '/', [], ['content-type' => ['application/x-www-form-urlencoded']], 'email=a%40b.test&tags[]=x');

        self::assertSame('a@b.test', RequestData::input($request, 'email'));
        self::assertSame('x', RequestData::input($request, 'tags.0'));
    }

    public function testBearerToken(): void
    {
        self::assertSame('abc.def-1', RequestData::bearerToken(new Request('GET', '/', [], ['authorization' => ['Bearer abc.def-1']], '')));
        self::assertSame('abc', RequestData::bearerToken(new Request('GET', '/', [], ['authorization' => ['bearer abc']], '')));
        self::assertNull(RequestData::bearerToken(new Request('GET', '/', [], ['authorization' => ['Basic abc']], '')));
        self::assertNull(RequestData::bearerToken(new Request('GET', '/', [], [], '')));
    }

    /**
     * @param array<string, mixed> $body
     * @param array<string, mixed> $query
     */
    private function json(array $body, array $query): Request
    {
        return new Request('POST', '/', $query, ['content-type' => ['application/json']], json_encode($body, JSON_THROW_ON_ERROR));
    }
}
