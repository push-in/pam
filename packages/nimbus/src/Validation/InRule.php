<?php

declare(strict_types=1);

namespace Nimbus\Validation;

/** `Rule::in(['draft', 'published'])`: the value (or each array item) is one of the values. */
final readonly class InRule implements ValidationRule
{
    /** @param list<string|int|float|bool|\BackedEnum> $values */
    public function __construct(private array $values, private bool $negate = false)
    {
    }

    public function validate(string $field, mixed $value): ?string
    {
        $allowed = array_map(
            static fn (mixed $item): string => match (true) {
                $item instanceof \BackedEnum => (string) $item->value,
                is_bool($item) => $item ? '1' : '0',
                default => (string) $item,
            },
            $this->values,
        );
        $values = is_array($value) ? $value : [$value];
        foreach ($values as $item) {
            if (is_bool($item)) {
                $item = $item ? '1' : '0';
            }
            $found = is_scalar($item) && in_array((string) $item, $allowed, true);
            if ($found === $this->negate) {
                return "The selected {$field} is invalid.";
            }
        }
        return null;
    }
}
