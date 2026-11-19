<?php

declare(strict_types=1);

namespace Nimbus\Security;

/**
 * Authenticated encryption (XSalsa20-Poly1305, ext-sodium) with the application key.
 * Payloads are `v1.` + base64url(nonce || ciphertext); a tampered payload fails to decrypt.
 */
final readonly class Crypt
{
    private string $key;

    public function __construct(#[\SensitiveParameter] string $key)
    {
        if (strlen($key) !== SODIUM_CRYPTO_SECRETBOX_KEYBYTES) {
            throw new \InvalidArgumentException('The encryption key must be 32 bytes.');
        }
        $this->key = $key;
    }

    /** Reads APP_KEY as `base64:...` (generate one with Crypt::generateKey()). */
    public static function fromEnvironment(): self
    {
        return new self(self::keyFromEnvironment());
    }

    /** The raw bytes of APP_KEY, shared with `UrlSigner`. */
    public static function keyFromEnvironment(): string
    {
        $value = getenv('APP_KEY');
        if (!is_string($value) || !str_starts_with($value, 'base64:')) {
            throw new \RuntimeException('APP_KEY must be set as base64:<32 random bytes>.');
        }
        $key = base64_decode(substr($value, 7), true);
        if ($key === false) {
            throw new \RuntimeException('APP_KEY is not valid base64.');
        }
        return $key;
    }

    public static function generateKey(): string
    {
        return 'base64:' . base64_encode(sodium_crypto_secretbox_keygen());
    }

    public function encrypt(#[\SensitiveParameter] string $plaintext): string
    {
        $nonce = random_bytes(SODIUM_CRYPTO_SECRETBOX_NONCEBYTES);
        return 'v1.' . self::encode($nonce . sodium_crypto_secretbox($plaintext, $nonce, $this->key));
    }

    public function decrypt(string $payload): string
    {
        $raw = str_starts_with($payload, 'v1.') ? self::decode(substr($payload, 3)) : null;
        if ($raw === null || strlen($raw) < SODIUM_CRYPTO_SECRETBOX_NONCEBYTES + SODIUM_CRYPTO_SECRETBOX_MACBYTES) {
            throw new DecryptException('The payload is not a Nimbus encrypted value.');
        }
        $plaintext = sodium_crypto_secretbox_open(
            substr($raw, SODIUM_CRYPTO_SECRETBOX_NONCEBYTES),
            substr($raw, 0, SODIUM_CRYPTO_SECRETBOX_NONCEBYTES),
            $this->key,
        );
        if ($plaintext === false) {
            throw new DecryptException('The payload was modified or encrypted with another key.');
        }
        return $plaintext;
    }

    private static function encode(string $bytes): string
    {
        return rtrim(strtr(base64_encode($bytes), '+/', '-_'), '=');
    }

    private static function decode(string $text): ?string
    {
        $bytes = base64_decode(strtr($text, '-_', '+/'), true);
        return $bytes === false ? null : $bytes;
    }
}
