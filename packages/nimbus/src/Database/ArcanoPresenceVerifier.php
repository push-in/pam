<?php

declare(strict_types=1);

namespace Nimbus\Database;

use Nimbus\Validation\PresenceVerifier;

/** Runs the `exists` and `unique` validation rules as `SELECT COUNT(*)` on the Arcano connections. */
final readonly class ArcanoPresenceVerifier implements PresenceVerifier
{
    public function __construct(private ArcanoManager $arcano)
    {
    }

    public function count(
        string $table,
        string $column,
        array $values,
        array $wheres = [],
        ?string $ignoreColumn = null,
        string|int|null $ignoreValue = null,
        ?string $connection = null,
    ): int {
        $query = $this->arcano->database($connection)->select()->from($table);
        $query = count($values) === 1 ? $query->where($column, $values[0]) : $query->where($column, 'IN', new \Arcano\Database\Injection\Parameter($values));
        foreach ($wheres as $where => $value) {
            $query = $value === null ? $query->where($where, null) : $query->where($where, $value);
        }
        if ($ignoreColumn !== null && $ignoreValue !== null) {
            $query = $query->where($ignoreColumn, '!=', $ignoreValue);
        }
        return $query->count();
    }
}
