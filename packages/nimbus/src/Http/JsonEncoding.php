<?php

declare(strict_types=1);

namespace Nimbus\Http;

use Pam\Http\Response;

/**
 * The `json_encode()` flags of every JSON response Nimbus writes: arrays returned by
 * actions, `JsonResource`, error bodies and `JsonEncoding::respond()`.
 *
 * The default keeps `JSON_UNESCAPED_UNICODE`, the PAM runtime's encoding. An app ported
 * from Laravel switches to Laravel's `JsonResponse` bytes (unicode and `/` escaped) once:
 *
 * ```php
 * $app->jsonEncoding(JsonEncoding::LARAVEL);
 * ```
 */
final class JsonEncoding
{
    /** The PAM runtime default: UTF-8 characters written as they are. */
    public const int DEFAULT = JSON_UNESCAPED_UNICODE;

    /** Laravel's `response()->json()` default: unicode as `\uXXXX`, `/` as `\/`. */
    public const int LARAVEL = 0;

    private static int $flags = self::DEFAULT;

    private function __construct()
    {
    }

    /** Sets the flags of the worker; `JSON_THROW_ON_ERROR` is always added. */
    public static function use(int $flags): void
    {
        self::$flags = $flags & ~JSON_THROW_ON_ERROR & ~JSON_PARTIAL_OUTPUT_ON_ERROR;
    }

    public static function flags(): int
    {
        return self::$flags;
    }

    /** @throws \JsonException */
    public static function encode(mixed $data, ?int $flags = null): string
    {
        return json_encode($data, ($flags ?? self::$flags) | JSON_THROW_ON_ERROR);
    }

    /** Writes `$data` as the JSON body with the given status and `application/json`. */
    public static function respond(
        Response $response,
        mixed $data,
        int $status = 200,
        ?int $flags = null,
        string $contentType = 'application/json; charset=utf-8',
    ): Response {
        return $response
            ->status($status)
            ->header('content-type', $contentType)
            ->send(self::encode($data, $flags));
    }
}
