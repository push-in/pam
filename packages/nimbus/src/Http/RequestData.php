<?php

declare(strict_types=1);

namespace Nimbus\Http;

use Pam\Http\Request;

/**
 * Laravel-style reads of the request input for code ported from Laravel
 * (`$request->input()`, `all()`, `has()`, `filled()`, `only()`, `bearerToken()`).
 *
 * The input is the form body (JSON object, urlencoded or the text parts of multipart)
 * over the query string: a body field wins over a query parameter of the same name.
 * Keys accept dot notation (`items.0.sku`). New code should prefer a typed
 * `Nimbus\Validation\Input` or a `FormRequest`; the `pushinbr/nimbus-rector` set
 * rewrites Laravel request calls to these methods.
 */
final class RequestData
{
    private function __construct()
    {
    }

    /** @return array<array-key, mixed> The body fields over the query parameters. */
    public static function all(Request $request): array
    {
        return array_replace($request->query(), FormData::parse($request));
    }

    /** One input value by key (dot notation), or every input with a null key. */
    public static function input(Request $request, ?string $key = null, mixed $default = null): mixed
    {
        $input = self::all($request);
        if ($key === null) {
            return $input;
        }
        return self::lookup($input, $key, $found) ?? ($found ? null : $default);
    }

    /** @param string|list<string> $keys Whether every key is present (dot notation), even with a null value. */
    public static function has(Request $request, string|array $keys): bool
    {
        $input = self::all($request);
        foreach ((array) $keys as $key) {
            self::lookup($input, $key, $found);
            if (!$found) {
                return false;
            }
        }
        return true;
    }

    /** Whether the key is present and not an empty string, an empty array or null. */
    public static function filled(Request $request, string $key): bool
    {
        $value = self::lookup(self::all($request), $key, $found);
        if (!$found || $value === null || $value === []) {
            return false;
        }
        return !is_string($value) || trim($value) !== '';
    }

    /**
     * @param string|list<string> ...$keys
     * @return array<string, mixed> The present keys only, keyed as asked.
     */
    public static function only(Request $request, string|array ...$keys): array
    {
        $input = self::all($request);
        $only = [];
        foreach ($keys as $key) {
            foreach ((array) $key as $name) {
                $value = self::lookup($input, $name, $found);
                if ($found) {
                    $only[$name] = $value;
                }
            }
        }
        return $only;
    }

    /** The token of an `Authorization: Bearer {token}` header, or null. */
    public static function bearerToken(Request $request): ?string
    {
        $authorization = $request->getHeader('authorization');
        if ($authorization === null || preg_match('/^Bearer\s+(\S+)\s*$/iD', $authorization, $matches) !== 1) {
            return null;
        }
        return $matches[1];
    }

    /**
     * @param array<array-key, mixed> $input
     * @param-out bool $found
     */
    private static function lookup(array $input, string $key, ?bool &$found = null): mixed
    {
        $found = true;
        if (array_key_exists($key, $input)) {
            return $input[$key];
        }
        $value = $input;
        foreach (explode('.', $key) as $segment) {
            if (!is_array($value) || !array_key_exists($segment, $value)) {
                $found = false;
                return null;
            }
            $value = $value[$segment];
        }
        return $value;
    }
}
