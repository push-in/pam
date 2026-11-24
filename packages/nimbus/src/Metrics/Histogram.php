<?php

declare(strict_types=1);

namespace Nimbus\Metrics;

final readonly class Histogram
{
    public function __construct(private MetricsRegistry $registry, public string $name)
    {
    }

    /** @param array<string, string> $labels */
    public function observe(float $value, array $labels = []): void
    {
        $this->registry->observe($this->name, $labels, $value);
    }
}
