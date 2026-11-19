<?php

declare(strict_types=1);

namespace Nimbus\Validation\Attributes;

/** @internal */
final class Size
{
    public static function of(mixed $value): int|float|null
    {
        return match (true) {
            is_int($value), is_float($value) => $value,
            is_string($value) => mb_strlen($value),
            is_array($value) => count($value),
            default => null,
        };
    }

    public static function message(string $field, mixed $value, string $bound): string
    {
        return match (true) {
            is_string($value) => "The {$field} field must have {$bound} characters.",
            is_array($value) => "The {$field} field must have {$bound} items.",
            default => "The {$field} field must be {$bound}.",
        };
    }
}
