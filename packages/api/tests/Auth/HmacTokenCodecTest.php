<?php

declare(strict_types=1);

namespace Pam\Api\Tests\Auth;

use Pam\Api\Auth\BearerTokenAuthenticator;
use Pam\Api\Auth\HmacTokenCodec;
use Pam\Api\Auth\MemoryTokenRevocationStore;
use Pam\Api\Auth\TokenPrincipal;
use Pam\Http\Request;
use PHPUnit\Framework\Attributes\CoversClass;
use PHPUnit\Framework\TestCase;

#[CoversClass(BearerTokenAuthenticator::class)]
#[CoversClass(HmacTokenCodec::class)]
#[CoversClass(MemoryTokenRevocationStore::class)]
#[CoversClass(TokenPrincipal::class)]
final class HmacTokenCodecTest extends TestCase
{
    public function testItIssuesAndAuthenticatesStrictBearerTokens(): void
    {
        $time = 1_800_000_000;
        $codec = self::codec($time);
        $token = $codec->issue('user-42', ['orders.read', 'orders.write']);
        $request = new Request(
            'GET',
            '/orders',
            [],
            ['authorization' => ["Bearer {$token}"]],
            '',
        );

        $principal = (new BearerTokenAuthenticator($codec))->authenticate($request);

        self::assertInstanceOf(TokenPrincipal::class, $principal);
        self::assertSame('user-42', $principal->identifier());
        self::assertTrue($principal->can('orders.write'));
        self::assertFalse($principal->can('admin'));
    }

    public function testItRejectsExpiredAndTamperedTokens(): void
    {
        $time = 1_800_000_000;
        $codec = self::codec($time);
        $token = $codec->issue('user-42', [], ttlSeconds: 10);
        $time += 20;

        self::assertNull($codec->verify($token));
        self::assertNull($codec->verify(substr($token, 0, -1) . 'x'));
    }

    public function testItRejectsAnotherIssuerOrAudience(): void
    {
        $time = 1_800_000_000;
        $token = self::codec($time)->issue('user-42', []);
        $other = new HmacTokenCodec(
            str_repeat('s', 32),
            'https://other.pam.dev',
            'other-api',
            clock: static function () use (&$time): int {
                return $time;
            },
        );

        self::assertNull($other->verify($token));
    }

    public function testItRequiresAProductionLengthSecret(): void
    {
        $this->expectException(\InvalidArgumentException::class);
        new HmacTokenCodec('short', 'https://issuer.pam.dev', 'pam-api');
    }

    public function testItRotatesKeysWithoutAcceptingUnknownKeyIdentifiers(): void
    {
        $time = 1_800_000_000;
        $old = new HmacTokenCodec(
            str_repeat('o', 32),
            'https://issuer.pam.dev',
            'pam-api',
            clock: static fn (): int => $time,
            keyIdentifier: '2026-01',
        );
        $new = new HmacTokenCodec(
            str_repeat('n', 32),
            'https://issuer.pam.dev',
            'pam-api',
            clock: static fn (): int => $time,
            keyIdentifier: '2026-02',
            verificationKeys: ['2026-01' => str_repeat('o', 32)],
        );

        self::assertInstanceOf(TokenPrincipal::class, $new->verify($old->issue('user-42', [])));
        self::assertNull(self::codec($time)->verify($old->issue('user-42', [])));
    }

    public function testBearerAuthenticationHonorsRevocationAndExpiryCleanup(): void
    {
        $time = 1_800_000_000;
        $codec = self::codec($time);
        $token = $codec->issue('user-42', []);
        $principal = $codec->verify($token);
        self::assertInstanceOf(TokenPrincipal::class, $principal);
        $revocations = new MemoryTokenRevocationStore(maximumEntries: 2);
        $revocations->revoke($principal->tokenIdentifier, $time + 900);
        $authenticator = new BearerTokenAuthenticator(
            $codec,
            $revocations,
            static function () use (&$time): int {
                return $time;
            },
        );

        self::assertNull($authenticator->authenticate(self::bearerRequest($token)));
        $time += 901;
        self::assertFalse($revocations->isRevoked($principal->tokenIdentifier, $time));
    }

    private static function codec(int &$time): HmacTokenCodec
    {
        return new HmacTokenCodec(
            str_repeat('s', 32),
            'https://issuer.pam.dev',
            'pam-api',
            clock: static function () use (&$time): int {
                return $time;
            },
        );
    }

    private static function bearerRequest(string $token): Request
    {
        return new Request('GET', '/', [], ['authorization' => ["Bearer {$token}"]], '');
    }
}
