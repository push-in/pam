<?php

declare(strict_types=1);

namespace Nimbus\Metrics;

final readonly class Counter
{
    public function __construct(private MetricsRegistry $registry, public string $name)
    {
    }

    /** @param array<string, string> $labels */
    public function inc(array $labels = [], int|float $by = 1): void
    {
        if ($by < 0) {
            throw new \InvalidArgumentException('A counter only increases.');
        }
        $this->registry->increment($this->name, $labels, 'sum', $by);
    }
}
