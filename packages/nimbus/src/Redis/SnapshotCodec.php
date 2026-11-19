<?php

declare(strict_types=1);

namespace Nimbus\Redis;

use Nimbus\Http\ResponseSnapshot;

/** @internal */
final class SnapshotCodec
{
    /** @param array<string, int|string> $extra */
    public static function encode(ResponseSnapshot $snapshot, array $extra = []): string
    {
        return json_encode($extra + [
            'status' => $snapshot->status,
            'headers' => $snapshot->headers,
            'body' => base64_encode($snapshot->body),
        ], JSON_THROW_ON_ERROR);
    }

    /** @return ?array{ResponseSnapshot, array<string, mixed>} */
    public static function decode(mixed $payload): ?array
    {
        if (!is_string($payload)) {
            return null;
        }
        try {
            $data = json_decode($payload, true, 8, JSON_THROW_ON_ERROR);
        } catch (\JsonException) {
            return null;
        }
        if (!is_array($data) || !is_int($data['status'] ?? null) || !is_array($data['headers'] ?? null) || !is_string($data['body'] ?? null)) {
            return null;
        }
        $body = base64_decode($data['body'], true);
        $headers = [];
        foreach ($data['headers'] as $name => $values) {
            if (!is_string($name) || !is_array($values)) {
                return null;
            }
            $headers[$name] = array_values(array_filter($values, is_string(...)));
        }
        if ($body === false) {
            return null;
        }
        return [new ResponseSnapshot($data['status'], $headers, $body), $data];
    }
}
