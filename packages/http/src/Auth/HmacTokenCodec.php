<?php

declare(strict_types=1);

namespace Pam\Api\Auth;

final readonly class HmacTokenCodec
{
    private \Closure $clock;

    /** @param array<array-key, mixed> $verificationKeys */
    public function __construct(
        #[\SensitiveParameter]
        private string $secret,
        private string $issuer,
        private string $audience,
        private int $leewaySeconds = 5,
        ?callable $clock = null,
        private string $keyIdentifier = 'primary',
        #[\SensitiveParameter]
        private array $verificationKeys = [],
    ) {
        if (strlen($secret) < 32) {
            throw new \InvalidArgumentException('The token signing secret must contain at least 32 bytes.');
        }
        if ($issuer === '' || $audience === '' || $leewaySeconds < 0 || $leewaySeconds > 300) {
            throw new \InvalidArgumentException('Token issuer, audience and leeway configuration are invalid.');
        }
        if (!self::validKeyIdentifier($keyIdentifier) || count($verificationKeys) > 4) {
            throw new \InvalidArgumentException('Token key identifiers or keyring size are invalid.');
        }
        foreach ($verificationKeys as $identifier => $verificationSecret) {
            if (!is_string($identifier) || !self::validKeyIdentifier($identifier)
                || !is_string($verificationSecret) || strlen($verificationSecret) < 32
                || $identifier === $keyIdentifier
            ) {
                throw new \InvalidArgumentException('Every verification key needs a unique identifier and 32-byte secret.');
            }
        }
        $this->clock = $clock === null
            ? static fn (): int => time()
            : \Closure::fromCallable($clock);
    }

    /** @param list<string> $abilities */
    public function issue(string $subject, array $abilities, int $ttlSeconds = 900): string
    {
        if ($subject === '' || $ttlSeconds < 1 || $ttlSeconds > 86_400) {
            throw new \InvalidArgumentException('Token subject and TTL are invalid.');
        }
        foreach ($abilities as $ability) {
            if ($ability === '') {
                throw new \InvalidArgumentException('Token abilities cannot contain empty values.');
            }
        }
        $now = ($this->clock)();
        $header = self::encodeJson(['alg' => 'HS256', 'typ' => 'JWT', 'kid' => $this->keyIdentifier]);
        $payload = self::encodeJson([
            'iss' => $this->issuer,
            'aud' => $this->audience,
            'sub' => $subject,
            'jti' => bin2hex(random_bytes(16)),
            'iat' => $now,
            'nbf' => $now,
            'exp' => $now + $ttlSeconds,
            'abilities' => array_values(array_unique($abilities)),
        ]);
        $signature = self::encode(hash_hmac('sha256', "{$header}.{$payload}", $this->secret, true));
        return "{$header}.{$payload}.{$signature}";
    }

    public function verify(string $token): ?TokenPrincipal
    {
        if ($token === '' || strlen($token) > 8_192) {
            return null;
        }
        $segments = explode('.', $token);
        if (count($segments) !== 3) {
            return null;
        }
        [$encodedHeader, $encodedPayload, $encodedSignature] = $segments;
        $header = self::decodeJson($encodedHeader);
        if ($header === null || ($header['alg'] ?? null) !== 'HS256' || ($header['typ'] ?? null) !== 'JWT') {
            return null;
        }
        $keyIdentifier = $header['kid'] ?? null;
        if (!is_string($keyIdentifier) || !self::validKeyIdentifier($keyIdentifier)) {
            return null;
        }
        $verificationSecret = $keyIdentifier === $this->keyIdentifier
            ? $this->secret
            : ($this->verificationKeys[$keyIdentifier] ?? null);
        if (!is_string($verificationSecret)) {
            return null;
        }
        $signature = self::decode($encodedSignature);
        if ($signature === null || !hash_equals(
            hash_hmac('sha256', "{$encodedHeader}.{$encodedPayload}", $verificationSecret, true),
            $signature,
        )) {
            return null;
        }
        $claims = self::decodeJson($encodedPayload);
        if ($claims === null) {
            return null;
        }
        if (!self::validStringClaim($claims, 'iss', $this->issuer)
            || !self::validStringClaim($claims, 'aud', $this->audience)
        ) {
            return null;
        }
        $subject = $claims['sub'] ?? null;
        $identifier = $claims['jti'] ?? null;
        $issuedAt = $claims['iat'] ?? null;
        $notBefore = $claims['nbf'] ?? null;
        $expiresAt = $claims['exp'] ?? null;
        $abilities = $claims['abilities'] ?? null;
        if (!is_string($subject) || $subject === '' || !is_string($identifier) || $identifier === ''
            || !is_int($issuedAt) || !is_int($notBefore) || !is_int($expiresAt) || !is_array($abilities)
        ) {
            return null;
        }
        $now = ($this->clock)();
        if ($issuedAt > $now + $this->leewaySeconds
            || $notBefore > $now + $this->leewaySeconds
            || $expiresAt <= $now - $this->leewaySeconds
            || $expiresAt <= $issuedAt
        ) {
            return null;
        }
        $validatedAbilities = [];
        foreach ($abilities as $ability) {
            if (!is_string($ability) || $ability === '') {
                return null;
            }
            $validatedAbilities[] = $ability;
        }
        return new TokenPrincipal($subject, array_values(array_unique($validatedAbilities)), $identifier);
    }

    /** @param array<string, mixed> $value */
    private static function encodeJson(array $value): string
    {
        return self::encode(json_encode($value, JSON_THROW_ON_ERROR | JSON_UNESCAPED_SLASHES));
    }

    private static function encode(string $value): string
    {
        return rtrim(strtr(base64_encode($value), '+/', '-_'), '=');
    }

    private static function decode(string $value): ?string
    {
        if ($value === '' || preg_match('/^[A-Za-z0-9_-]+$/D', $value) !== 1) {
            return null;
        }
        $padding = (4 - strlen($value) % 4) % 4;
        $decoded = base64_decode(strtr($value . str_repeat('=', $padding), '-_', '+/'), true);
        return is_string($decoded) ? $decoded : null;
    }

    /** @return array<string, mixed>|null */
    private static function decodeJson(string $value): ?array
    {
        $decoded = self::decode($value);
        if ($decoded === null || strlen($decoded) > 16_384) {
            return null;
        }
        try {
            $data = json_decode($decoded, true, 16, JSON_THROW_ON_ERROR);
        } catch (\JsonException) {
            return null;
        }
        if (!is_array($data)) {
            return null;
        }
        $object = [];
        foreach ($data as $key => $value) {
            if (!is_string($key)) {
                return null;
            }
            $object[$key] = $value;
        }
        return $object;
    }

    /** @param array<string, mixed> $claims */
    private static function validStringClaim(array $claims, string $name, string $expected): bool
    {
        return isset($claims[$name]) && is_string($claims[$name]) && hash_equals($expected, $claims[$name]);
    }

    private static function validKeyIdentifier(string $identifier): bool
    {
        return preg_match('/^[a-z0-9][a-z0-9._-]{0,63}$/D', $identifier) === 1;
    }
}
