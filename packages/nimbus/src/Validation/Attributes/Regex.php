<?php

declare(strict_types=1);

namespace Nimbus\Validation\Attributes;

use Nimbus\Validation\ValidationRule;

#[\Attribute(\Attribute::TARGET_PROPERTY)]
final readonly class Regex implements ValidationRule
{
    public function __construct(public string $pattern)
    {
    }

    public function validate(string $field, mixed $value): ?string
    {
        return is_string($value) && preg_match($this->pattern, $value) !== 1
            ? "The {$field} field format is invalid."
            : null;
    }
}
