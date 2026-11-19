<?php

declare(strict_types=1);

namespace Nimbus\Validation\Attributes;

use Nimbus\Validation\ValidationRule;

#[\Attribute(\Attribute::TARGET_PROPERTY)]
final readonly class Email implements ValidationRule
{
    public function validate(string $field, mixed $value): ?string
    {
        return is_string($value) && filter_var($value, FILTER_VALIDATE_EMAIL) === false
            ? "The {$field} field must be a valid email address."
            : null;
    }
}
