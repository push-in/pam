<?php

declare(strict_types=1);

namespace Nimbus\Http;

use Nimbus\Security\Crypt;
use Nimbus\Security\DecryptException;

/**
 * Encrypts (`Crypt`) or signs (HMAC-SHA256) cookie values. The cookie name is bound into
 * each value, so a value cannot be moved from one cookie to another.
 */
final readonly class CookieCodec
{
    private string $signingKey;

    public function __construct(
        private Crypt $crypt,
        #[\SensitiveParameter] string $key,
    ) {
        if (strlen($key) < 32) {
            throw new \InvalidArgumentException('The cookie signing key must have at least 32 bytes.');
        }
        $this->signingKey = hash_hmac('sha256', 'nimbus-cookie-signature', $key, true);
    }

    /** Uses APP_KEY (`base64:...`); encrypted cookies of APP_PREVIOUS_KEYS still decrypt. */
    public static function fromEnvironment(): self
    {
        $key = Crypt::keyFromEnvironment();
        return new self(new Crypt($key, Crypt::previousKeysFromEnvironment()), $key);
    }

    public function encrypt(string $name, string $value): string
    {
        return $this->crypt->encrypt(self::bind($name, $value));
    }

    /** The plaintext, or null when the value was not encrypted for this cookie with this key. */
    public function decrypt(string $name, string $payload): ?string
    {
        try {
            return self::unbind($name, $this->crypt->decrypt($payload));
        } catch (DecryptException) {
            return null;
        }
    }

    /** `value.signature`: readable by the client, but tamper-evident. */
    public function sign(string $name, string $value): string
    {
        return $value . '.' . $this->signature($name, $value);
    }

    /** The value, or null when the signature does not match. */
    public function verify(string $name, string $payload): ?string
    {
        $separator = strrpos($payload, '.');
        if ($separator === false) {
            return null;
        }
        $value = substr($payload, 0, $separator);
        return hash_equals($this->signature($name, $value), substr($payload, $separator + 1)) ? $value : null;
    }

    private function signature(string $name, string $value): string
    {
        return rtrim(strtr(base64_encode(hash_hmac('sha256', self::bind($name, $value), $this->signingKey, true)), '+/', '-_'), '=');
    }

    private static function bind(string $name, string $value): string
    {
        return hash('sha256', $name, true) . $value;
    }

    private static function unbind(string $name, string $plaintext): ?string
    {
        $prefix = hash('sha256', $name, true);
        return str_starts_with($plaintext, $prefix) ? substr($plaintext, strlen($prefix)) : null;
    }
}
