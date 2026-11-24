<?php

declare(strict_types=1);

namespace Nimbus\Metrics;

use Pam\Shared\Table;

/**
 * Series shared by every worker of a PAM cluster through a named
 * `Pam\Shared\Table`. The master owns the memory, so counters add up across
 * workers and survive worker recycling and reloads; any worker renders the
 * totals of the whole instance. Updates are atomic increments in shared
 * memory: no lock, no I/O.
 */
final class SharedTableMetricStore implements MetricStore
{
    public const int NAME_BYTES = 128;
    public const int LABEL_BYTES = 512;

    private ?Table $table = null;

    public function __construct(
        private readonly string $tableName = 'nimbus.metrics',
        private readonly int $maximumSeries = 4096,
    ) {
    }

    public static function available(): bool
    {
        return class_exists(Table::class) && Table::available();
    }

    public function describe(string $key, string $name, string $labels): void
    {
        if (strlen($name) > self::NAME_BYTES || strlen($labels) > self::LABEL_BYTES) {
            return;
        }
        $this->table()->set($key, ['name' => $name, 'labels' => $labels]);
    }

    public function add(string $key, string $column, int|float $amount): void
    {
        $this->table()->incr($key, $column, $amount);
    }

    public function put(string $key, array $values): void
    {
        $this->table()->set($key, $values);
    }

    public function rows(): iterable
    {
        foreach ($this->table() as $key => $row) {
            yield (string) $key => $row;
        }
    }

    private function table(): Table
    {
        if ($this->table !== null) {
            return $this->table;
        }
        $table = new Table($this->maximumSeries, 0.2, $this->tableName, 32);
        $table->column('name', Table::TYPE_STRING, self::NAME_BYTES);
        $table->column('labels', Table::TYPE_STRING, self::LABEL_BYTES);
        $table->column('value', Table::TYPE_FLOAT);
        $table->column('sum', Table::TYPE_FLOAT);
        $table->column('count', Table::TYPE_INT);
        $table->column('updated', Table::TYPE_INT);
        for ($bucket = 0; $bucket < MetricsRegistry::MAXIMUM_BUCKETS; ++$bucket) {
            $table->column('b' . $bucket, Table::TYPE_INT);
        }
        $table->create();
        return $this->table = $table;
    }
}
