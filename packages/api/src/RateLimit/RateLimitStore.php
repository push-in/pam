<?php

declare(strict_types=1);

namespace Pam\Api\RateLimit;

interface RateLimitStore
{
    public function consume(
        string $key,
        int $requestsPerSecond,
        int $capacity,
        float $now,
    ): RateLimitDecision;
}

