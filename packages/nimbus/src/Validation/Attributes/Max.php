<?php

declare(strict_types=1);

namespace Nimbus\Validation\Attributes;

use Nimbus\Validation\ValidationRule;

/** Maximum number, string length (characters) or item count. */
#[\Attribute(\Attribute::TARGET_PROPERTY)]
final readonly class Max implements ValidationRule
{
    public function __construct(public int|float $value)
    {
    }

    public function validate(string $field, mixed $value): ?string
    {
        $size = Size::of($value);
        return $size !== null && $size > $this->value ? Size::message($field, $value, "at most {$this->value}") : null;
    }
}
