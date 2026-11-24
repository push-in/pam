<?php

declare(strict_types=1);

namespace Nimbus\Validation;

/** @internal `connection.table` of the database rules. */
final class TableReference
{
    /** @return array{?string, string} */
    public static function parse(string $table): array
    {
        if ($table === '') {
            throw new \InvalidArgumentException('A database validation rule needs a table.');
        }
        if (class_exists($table)) {
            throw new \InvalidArgumentException('Database validation rules take a table name, not an entity class.');
        }
        $parts = explode('.', $table, 2);
        return count($parts) === 2 ? [$parts[0], $parts[1]] : [null, $table];
    }
}
