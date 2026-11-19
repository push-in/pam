<?php

declare(strict_types=1);

namespace Nimbus\Auth;

use Firebase\JWT\JWK;
use Firebase\JWT\JWT;
use Firebase\JWT\Key;

/**
 * JSON Web Tokens on firebase/php-jwt (optional dependency): HS256/384/512 with a shared secret,
 * RS256 and ES256 with key pairs, or verification only against a JWKS (e.g. an identity
 * provider's, through `Firebase\JWT\CachedKeySet`). Every key is bound to its algorithm, so a
 * token cannot choose another one; `iss`, `aud`, `sub` and `exp` are required.
 *
 * Abilities travel in the standard `scope` claim (space separated); `scp` and `abilities`
 * arrays are read too. Time claims are checked by firebase/php-jwt against the system clock
 * with its `JWT::$leeway`. Tokens must name their key with `kid`.
 */
final readonly class JwtCodec implements AccessTokenIssuer, AccessTokenVerifier
{
    private const array SYMMETRIC = ['HS256', 'HS384', 'HS512'];

    private const array ASYMMETRIC = ['RS256', 'RS384', 'RS512', 'ES256', 'ES384', 'EdDSA'];

    /**
     * @param array<string, Key>|\ArrayAccess<string, Key> $keys Verification keys by `kid`.
     * @param array{string, string|\OpenSSLAsymmetricKey, string}|null $signing `kid`, key material and algorithm.
     */
    private function __construct(
        private array|\ArrayAccess $keys,
        private ?array $signing,
        private string $issuer,
        private string $audience,
    ) {
        if ($issuer === '' || $audience === '') {
            throw new \InvalidArgumentException('JWT issuer and audience are required.');
        }
    }

    /** Shared-secret tokens; the secret needs as many bytes as the hash (32 for HS256). */
    public static function hmac(
        #[\SensitiveParameter]
        string $secret,
        string $issuer,
        string $audience,
        string $algorithm = 'HS256',
        string $keyIdentifier = 'primary',
    ): self {
        self::requireLibrary();
        if (!in_array($algorithm, self::SYMMETRIC, true)) {
            throw new \InvalidArgumentException('HMAC JWTs use HS256, HS384 or HS512.');
        }
        $minimum = ['HS256' => 32, 'HS384' => 48, 'HS512' => 64][$algorithm];
        if (strlen($secret) < $minimum) {
            throw new \InvalidArgumentException("The {$algorithm} signing secret must contain at least {$minimum} bytes.");
        }
        self::assertKeyIdentifier($keyIdentifier);
        return new self(
            [$keyIdentifier => new Key($secret, $algorithm)],
            [$keyIdentifier, $secret, $algorithm],
            $issuer,
            $audience,
        );
    }

    /**
     * Key-pair tokens (RS256, ES256, ...). Without a private key the codec only verifies.
     *
     * @param string $publicKey PEM public key or certificate.
     * @param ?string $privateKey PEM private key used by issue().
     */
    public static function asymmetric(
        string $publicKey,
        #[\SensitiveParameter]
        ?string $privateKey,
        string $issuer,
        string $audience,
        string $algorithm = 'RS256',
        string $keyIdentifier = 'primary',
    ): self {
        self::requireLibrary();
        if (!in_array($algorithm, self::ASYMMETRIC, true)) {
            throw new \InvalidArgumentException('Key-pair JWTs use RS256/384/512, ES256/384 or EdDSA.');
        }
        self::assertKeyIdentifier($keyIdentifier);
        $signing = null;
        if ($privateKey !== null) {
            if ($algorithm === 'EdDSA') {
                $signing = [$keyIdentifier, $privateKey, $algorithm];
            } else {
                $key = openssl_pkey_get_private($privateKey);
                if ($key === false) {
                    throw new \InvalidArgumentException('The JWT private key cannot be read.');
                }
                $signing = [$keyIdentifier, $key, $algorithm];
            }
        }
        return new self([$keyIdentifier => new Key($publicKey, $algorithm)], $signing, $issuer, $audience);
    }

    /**
     * Verifies tokens signed by the keys of a JSON Web Key Set; issue() is not available.
     *
     * @param array<array-key, mixed> $jwks The decoded `{"keys": [...]}` document.
     */
    public static function fromJwks(array $jwks, string $issuer, string $audience, ?string $defaultAlgorithm = null): self
    {
        self::requireLibrary();
        try {
            $keys = JWK::parseKeySet($jwks, $defaultAlgorithm);
        } catch (\UnexpectedValueException | \DomainException | \InvalidArgumentException $error) {
            throw new \InvalidArgumentException('The JSON Web Key Set is invalid.', 0, $error);
        }
        return new self($keys, null, $issuer, $audience);
    }

    /**
     * Verifies tokens against any key set by `kid`, such as `Firebase\JWT\CachedKeySet`, which
     * fetches and caches a remote JWKS through PSR-18 and PSR-6.
     *
     * @param \ArrayAccess<string, Key> $keys
     */
    public static function fromKeySet(\ArrayAccess $keys, string $issuer, string $audience): self
    {
        self::requireLibrary();
        return new self($keys, null, $issuer, $audience);
    }

    /**
     * @param list<string> $abilities
     * @param array<string, mixed> $claims Extra claims; registered claims cannot be overridden.
     */
    public function issue(string $subject, array $abilities, int $ttlSeconds = 900, array $claims = []): string
    {
        if ($this->signing === null) {
            throw new \LogicException('This JwtCodec only verifies tokens; configure a signing key to issue them.');
        }
        if ($subject === '' || $ttlSeconds < 1 || $ttlSeconds > 86_400) {
            throw new \InvalidArgumentException('Token subject and TTL are invalid.');
        }
        [$keyIdentifier, $key, $algorithm] = $this->signing;
        $now = time();
        $payload = [
            ...$claims,
            'iss' => $this->issuer,
            'aud' => $this->audience,
            'sub' => $subject,
            'jti' => bin2hex(random_bytes(16)),
            'iat' => $now,
            'nbf' => $now,
            'exp' => $now + $ttlSeconds,
            'scope' => implode(' ', Abilities::normalize($abilities)),
        ];
        return JWT::encode($payload, $key, $algorithm, $keyIdentifier);
    }

    public function verify(string $token): ?TokenPrincipal
    {
        if ($token === '' || strlen($token) > 8_192) {
            return null;
        }
        try {
            $payload = JWT::decode($token, $this->keys);
        } catch (\UnexpectedValueException | \DomainException | \InvalidArgumentException) {
            return null;
        }
        $claims = get_object_vars($payload);
        $audience = $claims['aud'] ?? null;
        $audiences = is_array($audience) ? $audience : [$audience];
        $matchesAudience = false;
        foreach ($audiences as $candidate) {
            $matchesAudience = $matchesAudience || (is_string($candidate) && hash_equals($this->audience, $candidate));
        }
        $issuer = $claims['iss'] ?? null;
        $subject = $claims['sub'] ?? null;
        $expiresAt = $claims['exp'] ?? null;
        if (!$matchesAudience || !is_string($issuer) || !hash_equals($this->issuer, $issuer)
            || !is_string($subject) || $subject === '' || !is_numeric($expiresAt)
        ) {
            return null;
        }
        $identifier = $claims['jti'] ?? null;
        if (!is_string($identifier) || $identifier === '') {
            $identifier = 'jwt:' . hash('sha256', $token);
        }
        $abilities = self::abilities($claims);
        return $abilities === null ? null : new TokenPrincipal($subject, $abilities, $identifier);
    }

    /**
     * @param array<string, mixed> $claims
     * @return list<string>|null
     */
    private static function abilities(array $claims): ?array
    {
        $scope = $claims['scope'] ?? null;
        $values = is_string($scope)
            ? preg_split('/\s+/', trim($scope), -1, PREG_SPLIT_NO_EMPTY)
            : ($claims['scp'] ?? $claims['abilities'] ?? []);
        if (!is_array($values)) {
            return null;
        }
        $abilities = [];
        foreach ($values as $ability) {
            if (!is_string($ability) || $ability === '') {
                return null;
            }
            $abilities[$ability] = true;
        }
        return array_keys($abilities);
    }

    private static function assertKeyIdentifier(string $identifier): void
    {
        if (preg_match('/^[A-Za-z0-9][A-Za-z0-9._-]{0,63}$/D', $identifier) !== 1) {
            throw new \InvalidArgumentException('JWT key identifiers must be short alphanumeric strings.');
        }
    }

    private static function requireLibrary(): void
    {
        if (!class_exists(JWT::class)) {
            throw new \LogicException('JWT support requires firebase/php-jwt: composer require firebase/php-jwt');
        }
    }
}
