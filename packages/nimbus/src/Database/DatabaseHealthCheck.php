<?php

declare(strict_types=1);

namespace Nimbus\Database;

use Nimbus\Health\HealthCheck;
use Nimbus\Health\HealthResult;
use Nimbus\Health\HealthState;

final readonly class DatabaseHealthCheck implements HealthCheck
{
    public function __construct(
        private ArcanoManager $arcano,
        private ?string $connection = null,
    ) {
    }

    public function check(): HealthResult
    {
        $startedAt = hrtime(true);
        try {
            $this->arcano->database($this->connection)->query('SELECT 1')->close();
            return new HealthResult(HealthState::Healthy, [
                'latency_ms' => self::elapsedMilliseconds($startedAt),
            ]);
        } catch (\Throwable $error) {
            return new HealthResult(HealthState::Unhealthy, [
                'latency_ms' => self::elapsedMilliseconds($startedAt),
                'error' => $error::class,
            ]);
        }
    }

    private static function elapsedMilliseconds(int $startedAt): float
    {
        return round((hrtime(true) - $startedAt) / 1_000_000, 3);
    }
}
