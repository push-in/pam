<?php

declare(strict_types=1);

namespace Nimbus\Metrics;

/** Series of this process only; for tests and single-process servers. */
final class MemoryMetricStore implements MetricStore
{
    /** @var array<string, array<string, int|float|string>> */
    private array $rows = [];

    public function __construct(private readonly int $maximumSeries = 4096)
    {
    }

    public function describe(string $key, string $name, string $labels): void
    {
        if ($this->reserve($key)) {
            $this->rows[$key]['name'] = $name;
            $this->rows[$key]['labels'] = $labels;
        }
    }

    public function add(string $key, string $column, int|float $amount): void
    {
        if ($this->reserve($key)) {
            $current = $this->rows[$key][$column] ?? 0;
            $this->rows[$key][$column] = (is_string($current) ? 0 : $current) + $amount;
        }
    }

    public function put(string $key, array $values): void
    {
        if ($this->reserve($key)) {
            foreach ($values as $column => $value) {
                $this->rows[$key][$column] = $value;
            }
        }
    }

    public function rows(): iterable
    {
        return $this->rows;
    }

    private function reserve(string $key): bool
    {
        if (isset($this->rows[$key])) {
            return true;
        }
        if (count($this->rows) >= $this->maximumSeries) {
            return false;
        }
        $this->rows[$key] = [];
        return true;
    }
}
