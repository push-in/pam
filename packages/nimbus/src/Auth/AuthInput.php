<?php

declare(strict_types=1);

namespace Nimbus\Auth;

use Nimbus\Validation\ValidationException;
use Pam\Http\Request;

/**
 * Reads the string fields of the auth routes from the query (GET) or the JSON body.
 *
 * @internal
 */
final readonly class AuthInput
{
    public static function string(Request $request, string $field, int $maximumLength = 1_024): string
    {
        $value = self::payload($request)[$field] ?? null;
        if (!is_string($value) || $value === '') {
            throw new ValidationException([$field => ["The {$field} field is required."]]);
        }
        if (strlen($value) > $maximumLength) {
            throw new ValidationException([$field => ["The {$field} field is too long."]]);
        }
        return $value;
    }

    /** @return array<array-key, mixed> */
    private static function payload(Request $request): array
    {
        if (in_array($request->method, ['GET', 'HEAD'], true)) {
            return $request->query();
        }
        if ($request->body() === '') {
            return [];
        }
        try {
            $decoded = $request->json();
        } catch (\JsonException) {
            $decoded = null;
        }
        if (!is_array($decoded)) {
            throw new ValidationException(['body' => ['The request body must be a JSON object.']]);
        }
        return $decoded;
    }
}
