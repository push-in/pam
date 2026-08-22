<?php

declare(strict_types=1);

namespace Pam\Api\Validation;

final readonly class EnumRule implements ValidationRule
{
    /** @param class-string<\BackedEnum> $enum */
    public function __construct(private string $enum)
    {
    }

    public function validate(string $field, mixed $value): ?string
    {
        foreach ($this->enum::cases() as $case) {
            if ($case->value === $value) {
                return null;
            }
        }
        return "The {$field} field is invalid.";
    }
}

