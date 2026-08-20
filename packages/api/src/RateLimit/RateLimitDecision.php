<?php

declare(strict_types=1);

namespace Pam\Api\RateLimit;

final readonly class RateLimitDecision
{
    public function __construct(
        public bool $allowed,
        public int $limit,
        public int $remaining,
        public int $retryAfterSeconds = 0,
    ) {
        if ($limit < 1 || $remaining < 0 || $retryAfterSeconds < 0) {
            throw new \InvalidArgumentException('Rate-limit decision values are invalid.');
        }
    }
}

