<?php

declare(strict_types=1);

namespace Nimbus\Session;

/** @internal JSON encoding of session data shared by the stores. */
final class SessionPayload
{
    /** @param array<string, mixed> $data */
    public static function encode(array $data): string
    {
        return json_encode($data, JSON_THROW_ON_ERROR | JSON_UNESCAPED_UNICODE | JSON_UNESCAPED_SLASHES | JSON_PRESERVE_ZERO_FRACTION);
    }

    /** @return array<string, mixed>|null */
    public static function decode(mixed $payload): ?array
    {
        if (!is_string($payload) || $payload === '') {
            return null;
        }
        try {
            $data = json_decode($payload, true, 64, JSON_THROW_ON_ERROR);
        } catch (\JsonException) {
            return null;
        }
        return is_array($data) ? self::normalize($data) : null;
    }

    /**
     * @param array<array-key, mixed> $data
     * @return array<string, mixed>
     */
    public static function normalize(array $data): array
    {
        $session = [];
        foreach ($data as $key => $value) {
            $session[(string) $key] = $value;
        }
        return $session;
    }

    public static function validId(string $id): bool
    {
        return preg_match('/^[a-f0-9]{40}$/D', $id) === 1;
    }
}
