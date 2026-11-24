<?php

declare(strict_types=1);

namespace Nimbus\Validation;

/** Counts rows for the `exists` and `unique` rules. */
interface PresenceVerifier
{
    /**
     * Rows of `$table` whose `$column` is one of `$values`, matching `$wheres` and
     * excluding the row whose `$ignoreColumn` is `$ignoreValue`.
     *
     * @param non-empty-list<scalar> $values
     * @param array<string, scalar|null> $wheres
     */
    public function count(
        string $table,
        string $column,
        array $values,
        array $wheres = [],
        ?string $ignoreColumn = null,
        string|int|null $ignoreValue = null,
        ?string $connection = null,
    ): int;
}
