<?php

declare(strict_types=1);

namespace Pam\Octane\Tests;

use Illuminate\Http\Request;
use Laravel\Octane\OctaneResponse;
use Pam\Http\Response as PamResponse;
use Pam\Octane\PamResponseEmitter;
use PHPUnit\Framework\TestCase;
use Symfony\Component\HttpFoundation\Cookie;
use Symfony\Component\HttpFoundation\Response;
use Symfony\Component\HttpFoundation\StreamedResponse;

final class PamResponseEmitterTest extends TestCase
{
    public function testItPreservesStatusHeadersCookiesAndBufferedOutput(): void
    {
        $request = Request::create('/created', 'POST');
        $source = new Response('{"created":true}', 201, [
            'Content-Type' => 'application/json',
            'X-Request-Id' => 'req_123',
        ]);
        $source->headers->setCookie(Cookie::create('session', 'secret')->withHttpOnly());
        $target = new PamResponse();

        (new PamResponseEmitter())->emit(
            $request,
            $target,
            new OctaneResponse($source, 'prefix:'),
        );

        $export = $target->export();
        self::assertSame(201, $export['status']);
        self::assertSame(['application/json'], $export['headers']['content-type']);
        self::assertSame(['req_123'], $export['headers']['x-request-id']);
        self::assertCount(1, $export['headers']['set-cookie']);
        self::assertSame('prefix:{"created":true}', $export['body']);
    }

    public function testItStreamsWithoutAccumulatingTheBody(): void
    {
        $request = Request::create('/stream');
        $source = new StreamedResponse(static function (): void {
            echo 'first';
            echo 'second';
        });
        $target = new PamResponse();

        (new PamResponseEmitter())->emit($request, $target, new OctaneResponse($source));

        $export = $target->export();
        self::assertSame('', $export['body']);
        self::assertSame('firstsecond', implode('', $export['chunks']));
    }

    public function testAHeadResponseNeverEmitsABody(): void
    {
        $request = Request::create('/download', 'HEAD');
        $target = new PamResponse();

        (new PamResponseEmitter())->emit(
            $request,
            $target,
            new OctaneResponse(new Response('must not leak')),
        );

        self::assertSame('', $target->export()['body']);
    }

    public function testBodylessStatusesNeverEmitCapturedOrResponseContent(): void
    {
        foreach ([204, 304] as $status) {
            $request = Request::create('/bodyless');
            $target = new PamResponse();

            (new PamResponseEmitter())->emit(
                $request,
                $target,
                new OctaneResponse(new Response('must not leak', $status), 'captured output'),
            );

            self::assertSame($status, $target->export()['status']);
            self::assertSame('', $target->export()['body']);
        }
    }

    public function testItDropsHopByHopHeadersAndAnInvalidatedContentLength(): void
    {
        $request = Request::create('/buffered');
        $source = new Response('response', 200, [
            'Connection' => 'keep-alive',
            'Transfer-Encoding' => 'chunked',
            'Content-Length' => '8',
        ]);
        $target = new PamResponse();

        (new PamResponseEmitter())->emit(
            $request,
            $target,
            new OctaneResponse($source, 'prefix:'),
        );

        $export = $target->export();
        self::assertArrayNotHasKey('connection', $export['headers']);
        self::assertArrayNotHasKey('transfer-encoding', $export['headers']);
        self::assertArrayNotHasKey('content-length', $export['headers']);
        self::assertSame('prefix:response', $export['body']);
    }

    public function testThrowableDetailsAreHiddenOutsideDebugMode(): void
    {
        $request = Request::create('/failure');
        $target = new PamResponse();

        (new PamResponseEmitter())->emitThrowable(
            $request,
            $target,
            'database password leaked',
            false,
        );

        $export = $target->export();
        self::assertSame(500, $export['status']);
        self::assertSame('Internal Server Error', $export['body']);
        self::assertStringNotContainsString('password', $export['body']);
    }

    public function testItEmitsAnInternalRouteTemplateWhenEnabled(): void
    {
        $request = Request::create('/users/42');
        $target = new PamResponse();

        (new PamResponseEmitter())->emit(
            $request,
            $target,
            new OctaneResponse(new Response('user')),
            '/users/{user}',
        );

        self::assertSame(
            ['/users/{user}'],
            $target->export()['headers']['x-pam-route-template'],
        );
    }
}
