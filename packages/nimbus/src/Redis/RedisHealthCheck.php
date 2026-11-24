<?php

declare(strict_types=1);

namespace Nimbus\Redis;

use Nimbus\Health\HealthCheck;
use Nimbus\Health\HealthResult;
use Nimbus\Health\HealthState;

/** Readiness check that sends `PING` to Redis. */
final readonly class RedisHealthCheck implements HealthCheck
{
    public function __construct(private Connection $redis)
    {
    }

    public function check(): HealthResult
    {
        $startedAt = hrtime(true);
        try {
            $reply = $this->redis->command(['PING']);
            $healthy = $reply === 'PONG' || $reply === true;
            return new HealthResult($healthy ? HealthState::Healthy : HealthState::Unhealthy, [
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
