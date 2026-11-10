<?php

declare(strict_types=1);

namespace Pam\Octane\Tables;

/**
 * Builds Octane's configured tables (`octane.tables`) and cache table
 * (`octane.cache`) on PAM shared memory, exactly as Octane's Swoole server
 * builds them. Every worker of a PAM cluster opens the same tables.
 */
final class PamTables
{
    public static function available(): bool
    {
        return class_exists(\Pam\Shared\Table::class, false) && \Pam\Shared\Table::available();
    }

    /**
     * @param array<mixed> $definitions `octane.tables`
     * @return array<string, PamTable>
     */
    public static function fromConfig(array $definitions): array
    {
        $tables = [];
        foreach ($definitions as $name => $columns) {
            $parts = explode(':', (string) $name);
            $table = new PamTable((int) ($parts[1] ?? 1000), self::segmentName('octane.table', $parts[0]));
            foreach (is_array($columns) ? $columns : [] as $column => $definition) {
                $definition = explode(':', is_string($definition) ? $definition : 'string');
                $table->column((string) $column, match ($definition[0]) {
                    'int' => PamTable::TYPE_INT,
                    'float' => PamTable::TYPE_FLOAT,
                    default => PamTable::TYPE_STRING,
                }, (int) ($definition[1] ?? 1000));
            }
            $table->create();
            $tables[$parts[0]] = $table;
        }

        return $tables;
    }

    /** @param array<mixed> $cache `octane.cache` */
    public static function cache(array $cache): PamTable
    {
        $rows = $cache['rows'] ?? 1000;
        $bytes = $cache['bytes'] ?? 10000;
        $table = new PamTable(is_numeric($rows) ? (int) $rows : 1000, 'octane.cache');
        $table->column('value', PamTable::TYPE_STRING, is_numeric($bytes) ? (int) $bytes : 10000);
        $table->column('expiration', PamTable::TYPE_INT);
        $table->create();

        return $table;
    }

    /** The same segment names as Octane's own PAM support uses. */
    private static function segmentName(string $prefix, string $name): string
    {
        return "{$prefix}.".sha1($name);
    }
}
