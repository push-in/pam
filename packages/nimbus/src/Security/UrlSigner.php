<?php

declare(strict_types=1);

namespace Nimbus\Security;

/**
 * Signs URL paths and query strings with HMAC-SHA256 under a key derived from APP_KEY.
 * The signature covers the path and every query parameter, so a signed URL cannot be
 * redirected to another route or have its parameters (expiry included) changed.
 */
final readonly class UrlSigner
{
    public const string SIGNATURE = 'signature';

    public const string EXPIRES = 'expires';

    private string $key;

    public function __construct(#[\SensitiveParameter] string $key)
    {
        if (strlen($key) < 32) {
            throw new \InvalidArgumentException('The URL signing key must be at least 32 bytes.');
        }
        // A derived key keeps signatures independent from the encryption use of APP_KEY.
        $this->key = hash_hmac('sha256', 'nimbus.url-signer.v1', $key, true);
    }

    /** Uses APP_KEY, as `Crypt::fromEnvironment()` does. */
    public static function fromEnvironment(): self
    {
        return new self(Crypt::keyFromEnvironment());
    }

    /**
     * Returns `path?query&expires=...&signature=...`.
     *
     * @param array<string, string|int|float|bool> $query
     * @param ?int $expiresAt Unix timestamp; null signs a URL that does not expire.
     */
    public function sign(string $path, array $query = [], ?int $expiresAt = null): string
    {
        unset($query[self::SIGNATURE], $query[self::EXPIRES]);
        if ($expiresAt !== null) {
            $query[self::EXPIRES] = $expiresAt;
        }
        $canonical = self::canonical($query);
        $query[self::SIGNATURE] = $this->signature($path, $canonical);
        return $path . '?' . self::canonical($query);
    }

    /**
     * Signs a URL valid for `$ttl` seconds.
     *
     * @param array<string, string|int|float|bool> $query
     */
    public function temporary(string $path, int $ttl, array $query = []): string
    {
        if ($ttl < 1) {
            throw new \InvalidArgumentException('A signed URL lifetime must be positive.');
        }
        return $this->sign($path, $query, time() + $ttl);
    }

    /** @param array<string, mixed> $query The request query, signature included. */
    public function check(string $path, array $query, ?int $now = null): UrlSignatureState
    {
        $signature = $query[self::SIGNATURE] ?? null;
        unset($query[self::SIGNATURE]);
        if (!is_string($signature) || $signature === '') {
            return UrlSignatureState::Invalid;
        }
        foreach ($query as $value) {
            if (!is_scalar($value)) {
                return UrlSignatureState::Invalid;
            }
        }
        /** @var array<string, string|int|float|bool> $query */
        if (!hash_equals($this->signature($path, self::canonical($query)), $signature)) {
            return UrlSignatureState::Invalid;
        }
        $expires = $query[self::EXPIRES] ?? null;
        if ($expires === null) {
            return UrlSignatureState::Valid;
        }
        if (!is_numeric($expires)) {
            return UrlSignatureState::Invalid;
        }
        return (int) $expires < ($now ?? time()) ? UrlSignatureState::Expired : UrlSignatureState::Valid;
    }

    /** @param array<string, mixed> $query */
    public function verify(string $path, array $query, ?int $now = null): bool
    {
        return $this->check($path, $query, $now) === UrlSignatureState::Valid;
    }

    private function signature(string $path, string $canonicalQuery): string
    {
        $mac = hash_hmac('sha256', $path . "\n" . $canonicalQuery, $this->key, true);
        return rtrim(strtr(base64_encode($mac), '+/', '-_'), '=');
    }

    /** @param array<string, string|int|float|bool> $query */
    private static function canonical(array $query): string
    {
        ksort($query, SORT_STRING);
        return http_build_query($query, '', '&', PHP_QUERY_RFC3986);
    }
}
