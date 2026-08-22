<?php

declare(strict_types=1);

namespace Pam\Api\Database;

use Pam\Api\Health\HealthCheck;
use Pam\Api\Health\HealthResult;
use Pam\Api\Health\HealthState;

final readonly class DatabaseHealthCheck implements HealthCheck
{
    public function __construct(
        private EloquentManager $eloquent,
        private ?string $connection = null,
    ) {
    }

    public function check(): HealthResult
    {
        $startedAt = hrtime(true);
        try {
            $this->eloquent->connection($this->connection)->select('SELECT 1');
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
