<?php

declare(strict_types=1);

namespace Nimbus\Tests\Auth;

use Firebase\JWT\JWT;
use Nimbus\Auth\JwtAuthenticator;
use Nimbus\Auth\JwtCodec;
use Nimbus\Auth\MemoryTokenRevocationStore;
use Pam\Http\Request;
use PHPUnit\Framework\Attributes\CoversClass;
use PHPUnit\Framework\TestCase;

#[CoversClass(JwtCodec::class)]
#[CoversClass(JwtAuthenticator::class)]
final class JwtCodecTest extends TestCase
{
    private const string SECRET = 'a-very-long-secret-of-at-least-32-bytes';

    public function testHmacTokensCarryScopesAndAreValidatedStrictly(): void
    {
        $codec = JwtCodec::hmac(self::SECRET, 'https://auth.test', 'api');
        $token = $codec->issue('user-1', ['products:read', 'products:write'], 600, ['tenant' => 'acme', 'sub' => 'ignored']);

        $principal = $codec->verify($token);
        self::assertNotNull($principal);
        self::assertSame('user-1', $principal->identifier());
        self::assertTrue($principal->can('products:write'));
        self::assertFalse($principal->can('orders:read'));

        self::assertNull(JwtCodec::hmac(self::SECRET, 'https://auth.test', 'other')->verify($token));
        self::assertNull(JwtCodec::hmac(self::SECRET, 'https://evil.test', 'api')->verify($token));
        self::assertNull(JwtCodec::hmac(str_repeat('x', 32), 'https://auth.test', 'api')->verify($token));
        self::assertNull($codec->verify($token . 'x'));
        self::assertNull($codec->verify('not.a.jwt'));
    }

    public function testUnsignedExpiredAndExpLessTokensAreRejected(): void
    {
        $codec = JwtCodec::hmac(self::SECRET, 'iss', 'aud');
        $claims = ['iss' => 'iss', 'aud' => 'aud', 'sub' => 'user-1', 'scope' => '*'];
        $none = rtrim(strtr(base64_encode('{"alg":"none","typ":"JWT"}'), '+/', '-_'), '=')
            . '.' . rtrim(strtr(base64_encode((string) json_encode($claims + ['exp' => time() + 60])), '+/', '-_'), '=') . '.';

        self::assertNull($codec->verify($none));
        self::assertNull($codec->verify(JWT::encode($claims + ['exp' => time() - 10], self::SECRET, 'HS256', 'primary')));
        self::assertNull($codec->verify(JWT::encode($claims, self::SECRET, 'HS256', 'primary')));
        self::assertNull($codec->verify(JWT::encode($claims + ['exp' => time() + 60], str_repeat('s', 64), 'HS512', 'primary')));
        self::assertNull($codec->verify(JWT::encode($claims + ['exp' => time() + 60], self::SECRET, 'HS256')), 'kid is required');
        self::assertNotNull($codec->verify(JWT::encode($claims + ['exp' => time() + 60, 'aud' => ['x', 'aud']], self::SECRET, 'HS256', 'primary')));
    }

    public function testRs256AndEs256KeyPairs(): void
    {
        foreach ([['RS256', ['private_key_type' => OPENSSL_KEYTYPE_RSA, 'private_key_bits' => 2048]],
            ['ES256', ['private_key_type' => OPENSSL_KEYTYPE_EC, 'curve_name' => 'prime256v1']]] as [$algorithm, $options]) {
            [$private, $public] = self::keyPair($options);
            $codec = JwtCodec::asymmetric($public, $private, 'iss', 'aud', $algorithm, 'k1');
            $token = $codec->issue('user-2', ['orders.read']);

            $verifier = JwtCodec::asymmetric($public, null, 'iss', 'aud', $algorithm, 'k1');
            self::assertSame('user-2', $verifier->verify($token)?->identifier(), $algorithm);
            [, $otherPublic] = self::keyPair($options);
            self::assertNull(JwtCodec::asymmetric($otherPublic, null, 'iss', 'aud', $algorithm)->verify($token));
        }
        // An RSA public key used as an HMAC secret (algorithm confusion) never verifies.
        [$private, $public] = self::keyPair(['private_key_type' => OPENSSL_KEYTYPE_RSA, 'private_key_bits' => 2048]);
        $forged = JWT::encode(['iss' => 'iss', 'aud' => 'aud', 'sub' => 'x', 'exp' => time() + 60], $public, 'HS256', 'primary');
        self::assertNull(JwtCodec::asymmetric($public, $private, 'iss', 'aud')->verify($forged));

        $this->expectException(\LogicException::class);
        JwtCodec::asymmetric($public, null, 'iss', 'aud')->issue('x', []);
    }

    public function testJwksVerification(): void
    {
        [$private, $public] = self::keyPair(['private_key_type' => OPENSSL_KEYTYPE_RSA, 'private_key_bits' => 2048]);
        $details = openssl_pkey_get_details(openssl_pkey_get_public($public) ?: throw new \RuntimeException());
        self::assertIsArray($details);
        $encode = static fn (string $value): string => rtrim(strtr(base64_encode($value), '+/', '-_'), '=');
        $jwks = ['keys' => [[
            'kty' => 'RSA',
            'kid' => 'idp-1',
            'alg' => 'RS256',
            'use' => 'sig',
            'n' => $encode($details['rsa']['n']),
            'e' => $encode($details['rsa']['e']),
        ]]];
        $token = JwtCodec::asymmetric($public, $private, 'https://idp.test', 'api', 'RS256', 'idp-1')->issue('user-3', ['a']);

        $principal = JwtCodec::fromJwks($jwks, 'https://idp.test', 'api')->verify($token);
        self::assertSame('user-3', $principal?->identifier());
        $unknownKid = JwtCodec::asymmetric($public, $private, 'https://idp.test', 'api', 'RS256', 'idp-2')->issue('user-3', ['a']);
        self::assertNull(JwtCodec::fromJwks($jwks, 'https://idp.test', 'api')->verify($unknownKid));
    }

    public function testTheAuthenticatorHonoursRevocation(): void
    {
        $codec = JwtCodec::hmac(self::SECRET, 'iss', 'aud');
        $revocations = new MemoryTokenRevocationStore();
        $authenticator = new JwtAuthenticator($codec, $revocations);
        $token = $codec->issue('user-1', ['*']);
        $request = new Request('GET', '/', [], ['authorization' => ["Bearer {$token}"]], '');

        $principal = $authenticator->authenticate($request);
        self::assertNotNull($principal);
        self::assertNull($authenticator->authenticate(new Request('GET', '/', [], [], '')));
        $revocations->revoke($codec->verify($token)->tokenIdentifier ?? '', time() + 900);
        self::assertNull($authenticator->authenticate($request));
    }

    public function testConfigurationErrors(): void
    {
        $this->expectException(\InvalidArgumentException::class);
        JwtCodec::hmac('short', 'iss', 'aud');
    }

    /**
     * @param array<string, mixed> $options
     * @return array{string, string}
     */
    private static function keyPair(array $options): array
    {
        $key = openssl_pkey_new($options);
        self::assertNotFalse($key);
        openssl_pkey_export($key, $private);
        $details = openssl_pkey_get_details($key);
        self::assertIsArray($details);
        self::assertIsString($private);
        self::assertIsString($details['key']);
        return [$private, $details['key']];
    }
}
