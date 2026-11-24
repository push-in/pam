<?php

declare(strict_types=1);

namespace Nimbus\Metrics;

/**
 * Where metric series live. One row per series: `name` and `labels` (the
 * rendered label set), plus numeric columns `value`, `sum`, `count`,
 * `updated` and the histogram buckets `b0` to `b15`.
 */
interface MetricStore
{
    /** Records the identity of a series; called once per series and process. */
    public function describe(string $key, string $name, string $labels): void;

    /** Adds to a numeric column, creating the series row when missing. */
    public function add(string $key, string $column, int|float $amount): void;

    /** @param array<string, int|float> $values Overwrites numeric columns. */
    public function put(string $key, array $values): void;

    /** @return iterable<string, array<string, int|float|string>> */
    public function rows(): iterable;
}
