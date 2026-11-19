<?php

declare(strict_types=1);

namespace Nimbus\Validation\Attributes;

use Nimbus\Validation\ValidationRule;

#[\Attribute(\Attribute::TARGET_PROPERTY)]
final readonly class In implements ValidationRule
{
    /** @var list<scalar> */
    public array $values;

    public function __construct(bool|float|int|string ...$values)
    {
        $this->values = array_values($values);
    }

    public function validate(string $field, mixed $value): ?string
    {
        return $value !== null && !in_array($value, $this->values, true)
            ? "The {$field} field must be one of: " . implode(', ', array_map(strval(...), $this->values)) . '.'
            : null;
    }
}
