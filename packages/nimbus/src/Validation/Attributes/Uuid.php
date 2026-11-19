<?php

declare(strict_types=1);

namespace Nimbus\Validation\Attributes;

use Nimbus\Validation\ValidationRule;

#[\Attribute(\Attribute::TARGET_PROPERTY)]
final readonly class Uuid implements ValidationRule
{
    public function validate(string $field, mixed $value): ?string
    {
        return is_string($value) && preg_match('/^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i', $value) !== 1
            ? "The {$field} field must be a valid UUID."
            : null;
    }
}
