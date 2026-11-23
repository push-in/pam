<?php

declare(strict_types=1);

namespace Nimbus\Security;

/**
 * Authenticated encryption (XSalsa20-Poly1305, ext-sodium) with the application key.
 * Payloads are `v1.` + base64url(nonce || ciphertext); a tampered payload fails to decrypt.
 *
 * Key rotation: values are always encrypted with the current key, and decryption also tries the
 * previous keys (`APP_PREVIOUS_KEYS`, comma-separated `base64:...`), so a new APP_KEY can be
 * deployed without invalidating what the old one encrypted. Drop a previous key once nothing
 * encrypted with it is still in use.
 */
final readonly class Crypt
{
    private string $key;

    /** @var list<string> */
    private array $previousKeys;

    /** @param list<string> $previousKeys Raw 32-byte keys still accepted by decrypt(). */
    public function __construct(#[\SensitiveParameter] string $key, #[\SensitiveParameter] array $previousKeys = [])
    {
        foreach ([$key, ...$previousKeys] as $candidate) {
            if (strlen($candidate) !== SODIUM_CRYPTO_SECRETBOX_KEYBYTES) {
                throw new \InvalidArgumentException('The encryption key must be 32 bytes.');
            }
        }
        $this->key = $key;
        $this->previousKeys = $previousKeys;
    }

    /** Reads APP_KEY as `base64:...` (generate one with Crypt::generateKey()) and APP_PREVIOUS_KEYS. */
    public static function fromEnvironment(): self
    {
        return new self(self::keyFromEnvironment(), self::previousKeysFromEnvironment());
    }

    /**
     * The raw bytes of each key in APP_PREVIOUS_KEYS (comma-separated `base64:...`).
     *
     * @return list<string>
     */
    public static function previousKeysFromEnvironment(): array
    {
        $value = getenv('APP_PREVIOUS_KEYS');
        if (!is_string($value) || trim($value) === '') {
            return [];
        }
        $keys = [];
        foreach (explode(',', $value) as $entry) {
            $entry = trim($entry);
            $key = str_starts_with($entry, 'base64:') ? base64_decode(substr($entry, 7), true) : false;
            if ($key === false) {
                throw new \RuntimeException('APP_PREVIOUS_KEYS must hold comma-separated base64:<32 random bytes> keys.');
            }
            $keys[] = $key;
        }
        return $keys;
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
        $nonce = substr($raw, 0, SODIUM_CRYPTO_SECRETBOX_NONCEBYTES);
        $ciphertext = substr($raw, SODIUM_CRYPTO_SECRETBOX_NONCEBYTES);
        foreach ([$this->key, ...$this->previousKeys] as $key) {
            $plaintext = sodium_crypto_secretbox_open($ciphertext, $nonce, $key);
            if ($plaintext !== false) {
                return $plaintext;
            }
        }
        throw new DecryptException('The payload was modified or encrypted with another key.');
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
