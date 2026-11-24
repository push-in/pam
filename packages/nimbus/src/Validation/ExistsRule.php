<?php

declare(strict_types=1);

namespace Nimbus\Validation;

/**
 * The value (or every value of an array) is in `$table.$column`:
 * `Rule::exists('products', 'sku')->where('active', true)`, or `'exists:products,sku'`.
 * `connection.table` picks the Arcano connection.
 */
final readonly class ExistsRule implements ValidationRule
{
    /** @param array<string, scalar|null> $wheres */
    public function __construct(
        private string $table,
        private string $column,
        private array $wheres = [],
    ) {
    }

    public function where(string $column, string|int|float|bool|null $value): self
    {
        return new self($this->table, $this->column, [...$this->wheres, $column => $value]);
    }

    public function passes(mixed $value): bool
    {
        $values = array_values(array_unique(array_filter(is_array($value) ? $value : [$value], is_scalar(...))));
        if ($values === [] || count($values) !== count(is_array($value) ? array_unique($value, SORT_REGULAR) : [$value])) {
            return false;
        }
        [$connection, $table] = TableReference::parse($this->table);
        return Validator::presenceVerifier()->count($table, $this->column, $values, $this->wheres, connection: $connection)
            >= count($values);
    }

    public function validate(string $field, mixed $value): ?string
    {
        return $this->passes($value) ? null : "The selected {$field} is invalid.";
    }
}
