<?php

declare(strict_types=1);

namespace Nimbus\Http;

use Pam\Http\Request;

/** The fields of a form body: urlencoded, the text parts of multipart/form-data, or a JSON object. */
final class FormData
{
    private const int MAXIMUM_BODY_BYTES = 8 * 1024 * 1024;

    /** @return array<array-key, mixed> */
    public static function parse(Request $request): array
    {
        $body = $request->body();
        if ($body === '' || strlen($body) > self::MAXIMUM_BODY_BYTES) {
            return [];
        }
        $type = strtolower($request->getHeader('content-type') ?? '');
        if (str_starts_with($type, 'application/x-www-form-urlencoded')) {
            parse_str($body, $fields);
            return $fields;
        }
        if (str_starts_with($type, 'multipart/form-data')) {
            return self::multipart($body, $request->getHeader('content-type') ?? '');
        }
        if (str_contains($type, 'json')) {
            try {
                $decoded = json_decode($body, true, 64, JSON_THROW_ON_ERROR);
            } catch (\JsonException) {
                return [];
            }
            return is_array($decoded) ? $decoded : [];
        }
        return [];
    }

    /** A single text field of the form, or null. */
    public static function field(Request $request, string $name): ?string
    {
        $value = self::parse($request)[$name] ?? null;
        return is_string($value) ? $value : null;
    }

    /** @return array<string, string> */
    private static function multipart(string $body, string $type): array
    {
        if (preg_match('/boundary="?([^";]+)"?/i', $type, $match) !== 1) {
            return [];
        }
        $fields = [];
        foreach (explode('--' . $match[1], $body) as $part) {
            $split = strpos($part, "\r\n\r\n");
            if ($split === false) {
                continue;
            }
            $headers = substr($part, 0, $split);
            if (stripos($headers, 'filename=') !== false
                || preg_match('/content-disposition:\s*form-data;[^\r\n]*\bname="([^"]*)"/i', $headers, $name) !== 1) {
                continue;
            }
            $value = substr($part, $split + 4);
            $fields[$name[1]] = str_ends_with($value, "\r\n") ? substr($value, 0, -2) : $value;
        }
        return $fields;
    }
}
