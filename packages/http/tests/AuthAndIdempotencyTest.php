<?php

declare(strict_types=1);

namespace Pam\Api\Tests;

use Pam\App;
use Pam\Api\Auth\Authenticator;
use Pam\Api\Auth\Principal;
use Pam\Api\Idempotency\MemoryIdempotencyStore;
use Pam\Api\Middleware\AuthenticateMiddleware;
use Pam\Api\Middleware\IdempotencyMiddleware;
use Pam\Http\Request;
use Pam\Http\Response;
use PHPUnit\Framework\TestCase;

final class AuthAndIdempotencyTest extends TestCase
{
    public function testAuthenticationPublishesPrincipalInRequestScope(): void
    {
        $app = new App(discoverPackages: false);
        $authenticator = new class implements Authenticator {
            public function authenticate(Request $request): ?Principal
            {
                return $request->getHeader('authorization') === 'Bearer valid'
                    ? new TestPrincipal('user-1')
                    : null;
            }
        };
        $app->middleware(new AuthenticateMiddleware($authenticator, $app->container()));
        $app->get('/me', [AuthController::class, 'show']);

        $unauthorized = $app->handle(new Request('GET', '/me', [], [], ''), new Response())->export();
        $authorized = $app->handle(new Request(
            'GET', '/me', [], ['authorization' => ['Bearer valid']], '',
        ), new Response())->export();

        self::assertSame(401, $unauthorized['status']);
        self::assertSame('{"id":"user-1"}', $authorized['body']);
    }

    public function testIdempotencyReplaysResponseAndRejectsAnotherPayload(): void
    {
        $app = new App(discoverPackages: false);
        $calls = 0;
        $app->post('/orders', static function (Request $request, Response $response) use (&$calls): Response {
            ++$calls;
            return $response->json(['sequence' => $calls], 201);
        })->middleware(new IdempotencyMiddleware(new MemoryIdempotencyStore()));

        $first = $app->handle($this->idempotentRequest('{"amount":10}'), new Response())->export();
        $replay = $app->handle($this->idempotentRequest('{"amount":10}'), new Response())->export();
        $conflict = $app->handle($this->idempotentRequest('{"amount":20}'), new Response())->export();

        self::assertSame(1, $calls);
        self::assertSame($first['body'], $replay['body']);
        self::assertSame(['true'], $replay['headers']['idempotency-replayed']);
        self::assertSame(409, $conflict['status']);
    }

    private function idempotentRequest(string $body): Request
    {
        return new Request('POST', '/orders', [], ['idempotency-key' => ['order-1']], $body);
    }
}

final readonly class TestPrincipal implements Principal
{
    public function __construct(private string $id)
    {
    }

    public function identifier(): string
    {
        return $this->id;
    }

    public function can(string $ability): bool
    {
        return $ability === 'profile.read';
    }
}

final class AuthController
{
    public function show(Principal $principal, Response $response): Response
    {
        return $response->json(['id' => $principal->identifier()]);
    }
}

