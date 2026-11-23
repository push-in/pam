<?php

declare(strict_types=1);

namespace Nimbus\Tests\Security;

use Nimbus\App;
use Nimbus\Http\ClientIpResolver;
use Nimbus\Http\HttpException;
use Nimbus\Http\ProblemCode;
use Nimbus\Idempotency\MemoryIdempotencyStore;
use Nimbus\Middleware\IdempotencyMiddleware;
use Nimbus\Middleware\SecurityHeadersMiddleware;
use Nimbus\Security\Crypt;
use Nimbus\Security\DecryptException;
use Nimbus\View\ViewFactory;
use Pam\Http\Request;
use Pam\Http\Response;
use PHPUnit\Framework\Attributes\CoversClass;
use PHPUnit\Framework\TestCase;

/** Regression tests of the 2026-11-20 audit findings B5, B7 and M9. */
#[CoversClass(SecurityHeadersMiddleware::class)]
#[CoversClass(ClientIpResolver::class)]
#[CoversClass(IdempotencyMiddleware::class)]
#[CoversClass(Crypt::class)]
#[CoversClass(ViewFactory::class)]
final class HardeningTest extends TestCase
{
    /** @var array<string, mixed> */
    private array $server = [];

    protected function setUp(): void
    {
        $this->server = $_SERVER;
    }

    protected function tearDown(): void
    {
        $_SERVER = $this->server;
    }

    public function testSecurityHeadersAreOnErrorResponsesToo(): void
    {
        $app = new App(discoverPackages: false);
        $app->middleware(new SecurityHeadersMiddleware(contentSecurityPolicy: SecurityHeadersMiddleware::DEFAULT_CSP));
        $app->get('/boom', static function (): never {
            throw new \RuntimeException('boom');
        });
        $app->get('/gone', static function (): never {
            throw new HttpException(410, ProblemCode::NotFound, 'Gone.');
        });
        $app->get('/own', static fn (Request $request, Response $response): Response => $response
            ->header('content-security-policy', "default-src 'none'")->send('ok'));

        foreach (['/boom' => 500, '/gone' => 410, '/missing' => 404] as $path => $status) {
            $exported = $app->handle(new Request('GET', $path, [], [], ''), new Response())->export();
            self::assertSame($status, $exported['status'], $path);
            self::assertSame(['nosniff'], $exported['headers']['x-content-type-options'] ?? null, $path);
            self::assertSame(['DENY'], $exported['headers']['x-frame-options'] ?? null, $path);
        }
        $own = $app->handle(new Request('GET', '/own', [], [], ''), new Response())->export();
        self::assertSame(["default-src 'none'"], $own['headers']['content-security-policy']);
    }

    public function testHstsBehindATrustedTlsProxyOnly(): void
    {
        $proto = new Request('GET', '/', [], ['x-forwarded-proto' => ['https']], '');
        $_SERVER['HTTPS'] = 'off';
        $_SERVER['REMOTE_ADDR'] = '10.0.0.5';
        self::assertTrue(self::hsts(new SecurityHeadersMiddleware(trustedProxies: ['10.0.0.0/8']), $proto));
        self::assertTrue(self::hsts(new SecurityHeadersMiddleware(trustForwardedProto: true), $proto));
        self::assertFalse(self::hsts(new SecurityHeadersMiddleware(), $proto));
        $_SERVER['REMOTE_ADDR'] = '203.0.113.9';
        self::assertFalse(self::hsts(new SecurityHeadersMiddleware(trustedProxies: ['10.0.0.0/8']), $proto), 'Forged by a client.');
        $_SERVER['HTTPS'] = 'on';
        self::assertTrue(self::hsts(new SecurityHeadersMiddleware(), new Request('GET', '/', [], [], '')));
    }

    public function testIdempotencyKeysAreScopedByCredential(): void
    {
        $app = new App(discoverPackages: false);
        $calls = 0;
        $app->post('/orders', static function (Request $request, Response $response) use (&$calls): Response {
            ++$calls;
            return $response->json(['order' => $calls, 'owner' => $request->getHeader('authorization')], 201);
        })->middleware(new IdempotencyMiddleware(new MemoryIdempotencyStore()));

        $alice = $app->handle(self::order('Bearer alice'), new Response())->export();
        $bob = $app->handle(self::order('Bearer bob'), new Response())->export();
        $aliceAgain = $app->handle(self::order('Bearer alice'), new Response())->export();

        self::assertSame(2, $calls, 'Bob reusing the key gets his own request, not Alice\'s response.');
        self::assertStringContainsString('bob', $bob['body']);
        self::assertSame($alice['body'], $aliceAgain['body']);
        self::assertSame(['true'], $aliceAgain['headers']['idempotency-replayed'] ?? null);
    }

    public function testCryptDecryptsWithPreviousKeysAndEncryptsWithTheCurrentOne(): void
    {
        $old = random_bytes(32);
        $new = random_bytes(32);
        $legacy = (new Crypt($old))->encrypt('secret');
        $rotated = new Crypt($new, [$old]);

        self::assertSame('secret', $rotated->decrypt($legacy));
        self::assertSame('secret', (new Crypt($new))->decrypt($rotated->encrypt('secret')));

        $previous = getenv('APP_PREVIOUS_KEYS');
        $previousKey = getenv('APP_KEY');
        putenv('APP_KEY=base64:' . base64_encode($new));
        putenv('APP_PREVIOUS_KEYS=base64:' . base64_encode(random_bytes(32)) . ', base64:' . base64_encode($old));
        try {
            self::assertSame('secret', Crypt::fromEnvironment()->decrypt($legacy));
        } finally {
            putenv($previous === false ? 'APP_PREVIOUS_KEYS' : 'APP_PREVIOUS_KEYS=' . $previous);
            putenv($previousKey === false ? 'APP_KEY' : 'APP_KEY=' . $previousKey);
        }

        $this->expectException(DecryptException::class);
        (new Crypt($new))->decrypt($legacy);
    }

    public function testInlineTemplatesAreCachedInABoundedLru(): void
    {
        $views = new ViewFactory(sys_get_temp_dir());
        $first = $views->renderString('first {{ $n }}', ['n' => 1]);
        self::assertSame('first 1', $first);
        for ($i = 0; $i < ViewFactory::INLINE_CACHE_LIMIT + 50; ++$i) {
            if ($i % 10 === 0) {
                // Recently used sources stay cached.
                $views->renderString('first {{ $n }}', ['n' => 1]);
            }
            self::assertSame("v{$i}", $views->renderString("v{$i}"));
        }
        self::assertSame(ViewFactory::INLINE_CACHE_LIMIT, $views->inlineCacheSize());
        self::assertTrue($views->exists('string:' . hash('xxh128', 'first {{ $n }}')));
        self::assertFalse($views->exists('string:' . hash('xxh128', 'v0')));
    }

    private static function hsts(SecurityHeadersMiddleware $middleware, Request $request): bool
    {
        $app = new App(discoverPackages: false);
        $app->middleware($middleware);
        $app->get('/', static fn (): string => 'ok');
        $exported = $app->handle($request, new Response())->export();
        return isset($exported['headers']['strict-transport-security']);
    }

    private static function order(string $authorization): Request
    {
        return new Request('POST', '/orders', [], [
            'idempotency-key' => ['order-1'],
            'authorization' => [$authorization],
        ], '{"amount":10}');
    }
}
