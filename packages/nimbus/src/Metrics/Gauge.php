<?php

declare(strict_types=1);

namespace Nimbus\Metrics;

final readonly class Gauge
{
    public function __construct(private MetricsRegistry $registry, public string $name)
    {
    }

    /** @param array<string, string> $labels */
    public function set(float $value, array $labels = []): void
    {
        $this->registry->set($this->name, $labels, $value);
    }
}
