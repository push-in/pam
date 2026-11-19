<?php

declare(strict_types=1);

namespace Nimbus\Validation\Attributes;

use Nimbus\Validation\ValidationRule;

/** An absolute http or https URL. */
#[\Attribute(\Attribute::TARGET_PROPERTY)]
final readonly class Url implements ValidationRule
{
    public function validate(string $field, mixed $value): ?string
    {
        if (!is_string($value)) {
            return null;
        }
        $scheme = strtolower((string) parse_url($value, PHP_URL_SCHEME));
        return filter_var($value, FILTER_VALIDATE_URL) === false || !in_array($scheme, ['http', 'https'], true)
            ? "The {$field} field must be a valid URL."
            : null;
    }
}
