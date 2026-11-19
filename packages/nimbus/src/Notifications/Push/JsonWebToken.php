<?php

declare(strict_types=1);

namespace Nimbus\Notifications\Push;

/**
 * Compact JWS signing with ext-openssl: RS256 (Google OAuth2) and ES256 (APNs).
 */
final class JsonWebToken
{
    private function __construct()
    {
    }

    /**
     * @param array<string, mixed> $claims
     * @param array<string, string> $header
     */
    public static function rs256(array $claims, #[\SensitiveParameter] string $privateKeyPem, array $header = []): string
    {
        return self::sign('RS256', $claims, $privateKeyPem, $header);
    }

    /**
     * @param array<string, mixed> $claims
     * @param array<string, string> $header
     */
    public static function es256(array $claims, #[\SensitiveParameter] string $privateKeyPem, array $header = []): string
    {
        return self::sign('ES256', $claims, $privateKeyPem, $header);
    }

    public static function base64Url(string $bytes): string
    {
        return rtrim(strtr(base64_encode($bytes), '+/', '-_'), '=');
    }

    /**
     * @param array<string, mixed> $claims
     * @param array<string, string> $header
     */
    private static function sign(string $algorithm, array $claims, string $privateKeyPem, array $header): string
    {
        if (!extension_loaded('openssl')) {
            throw new \LogicException('Push notifications require ext-openssl.');
        }
        $key = openssl_pkey_get_private($privateKeyPem);
        if ($key === false) {
            throw new \InvalidArgumentException('The push signing key is not a valid PEM private key.');
        }
        $expected = $algorithm === 'RS256' ? OPENSSL_KEYTYPE_RSA : OPENSSL_KEYTYPE_EC;
        $details = openssl_pkey_get_details($key);
        if (!is_array($details) || ($details['type'] ?? null) !== $expected) {
            throw new \InvalidArgumentException("The push signing key does not match {$algorithm}.");
        }
        $input = self::base64Url(json_encode(['alg' => $algorithm, 'typ' => 'JWT'] + $header, JSON_THROW_ON_ERROR))
            . '.' . self::base64Url(json_encode($claims, JSON_THROW_ON_ERROR | JSON_UNESCAPED_SLASHES));
        if (!openssl_sign($input, $signature, $key, OPENSSL_ALGO_SHA256) || !is_string($signature)) {
            throw new \RuntimeException('Could not sign the push token.');
        }
        if ($algorithm === 'ES256') {
            $signature = self::derToRaw($signature, 32);
        }
        return $input . '.' . self::base64Url($signature);
    }

    /** Converts an ASN.1 DER ECDSA signature to the fixed-size R || S form of JWS. */
    private static function derToRaw(string $der, int $size): string
    {
        $offset = 2;
        if (ord($der[1]) & 0x80) {
            $offset += ord($der[1]) & 0x7f;
        }
        $parts = [];
        for ($i = 0; $i < 2; ++$i) {
            if (ord($der[$offset]) !== 0x02) {
                throw new \RuntimeException('Invalid ECDSA signature.');
            }
            $length = ord($der[$offset + 1]);
            $value = ltrim(substr($der, $offset + 2, $length), "\0");
            $parts[] = str_pad($value, $size, "\0", STR_PAD_LEFT);
            $offset += 2 + $length;
        }
        return $parts[0] . $parts[1];
    }
}
