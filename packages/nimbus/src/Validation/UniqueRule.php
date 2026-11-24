<?php

declare(strict_types=1);

namespace Nimbus\Validation;

/**
 * No other row of `$table` has the value in `$column`:
 * `Rule::unique('users', 'email')->ignore($user->id)`, or `'unique:users,email,5,id'`.
 */
final readonly class UniqueRule implements ValidationRule
{
    /** @param array<string, scalar|null> $wheres */
    public function __construct(
        private string $table,
        private string $column,
        private string|int|null $ignore = null,
        private string $idColumn = 'id',
        private array $wheres = [],
    ) {
    }

    /** Skips the row being updated. */
    public function ignore(string|int|null $id, string $idColumn = 'id'): self
    {
        return new self($this->table, $this->column, $id, $idColumn, $this->wheres);
    }

    public function where(string $column, string|int|float|bool|null $value): self
    {
        return new self($this->table, $this->column, $this->ignore, $this->idColumn, [...$this->wheres, $column => $value]);
    }

    public function passes(mixed $value): bool
    {
        if (!is_scalar($value)) {
            return false;
        }
        [$connection, $table] = TableReference::parse($this->table);
        return Validator::presenceVerifier()->count(
            $table,
            $this->column,
            [$value],
            $this->wheres,
            $this->ignore === null ? null : $this->idColumn,
            $this->ignore,
            $connection,
        ) === 0;
    }

    public function validate(string $field, mixed $value): ?string
    {
        return $this->passes($value) ? null : "The {$field} has already been taken.";
    }
}
