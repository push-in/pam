<?php

declare(strict_types=1);

namespace Nimbus\Auth\OAuth;

/**
 * Key and payload encoding shared by the state stores.
 *
 * @internal
 */
final readonly class OAuthStateCodec
{
    public static function key(string $state): string
    {
        return hash('sha256', $state);
    }

    /** @param array<string, string> $data */
    public static function encode(array $data, int $expiresAt): string
    {
        return json_encode(['data' => $data, 'expiresAt' => $expiresAt], JSON_THROW_ON_ERROR | JSON_UNESCAPED_SLASHES);
    }

    /** @return array<string, string>|null */
    public static function decode(mixed $payload, int $now): ?array
    {
        if (!is_string($payload)) {
            return null;
        }
        try {
            $decoded = json_decode($payload, true, 4, JSON_THROW_ON_ERROR);
        } catch (\JsonException) {
            return null;
        }
        if (!is_array($decoded) || !is_int($decoded['expiresAt'] ?? null) || $decoded['expiresAt'] <= $now
            || !is_array($decoded['data'] ?? null)
        ) {
            return null;
        }
        $data = [];
        foreach ($decoded['data'] as $key => $value) {
            if (is_string($key) && is_string($value)) {
                $data[$key] = $value;
            }
        }
        return $data;
    }
}
