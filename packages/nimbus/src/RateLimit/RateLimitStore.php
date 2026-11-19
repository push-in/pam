<?php

declare(strict_types=1);

namespace Nimbus\RateLimit;

interface RateLimitStore
{
    public function consume(
        string $key,
        int $requestsPerSecond,
        int $capacity,
        float $now,
    ): RateLimitDecision;
}

