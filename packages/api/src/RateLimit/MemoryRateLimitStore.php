<?php

declare(strict_types=1);

namespace Pam\Api\RateLimit;

final class MemoryRateLimitStore implements RateLimitStore
{
    /** @var array<string, array{tokens: float, updatedAt: float}> */
    private array $buckets = [];

    public function __construct(
        private readonly int $maxBuckets = 65_536,
        private readonly float $idleTtlSeconds = 300.0,
    ) {
        if ($maxBuckets < 1 || $idleTtlSeconds <= 0) {
            throw new \InvalidArgumentException('Memory rate-limit store configuration is invalid.');
        }
    }

    public function consume(
        string $key,
        int $requestsPerSecond,
        int $capacity,
        float $now,
    ): RateLimitDecision {
        if (!isset($this->buckets[$key]) && count($this->buckets) >= $this->maxBuckets) {
            $threshold = $now - $this->idleTtlSeconds;
            $this->buckets = array_filter(
                $this->buckets,
                static fn (array $entry): bool => $entry['updatedAt'] >= $threshold,
            );
            if (count($this->buckets) >= $this->maxBuckets) {
                return new RateLimitDecision(false, $capacity, 0, 1);
            }
        }

        $bucket = $this->buckets[$key] ?? ['tokens' => (float) $capacity, 'updatedAt' => $now];
        $elapsed = max(0.0, $now - $bucket['updatedAt']);
        $tokens = min((float) $capacity, $bucket['tokens'] + ($elapsed * $requestsPerSecond));
        if ($tokens < 1.0) {
            $this->buckets[$key] = ['tokens' => $tokens, 'updatedAt' => $now];
            $retry = max(1, (int) ceil((1.0 - $tokens) / $requestsPerSecond));
            return new RateLimitDecision(false, $capacity, 0, $retry);
        }

        $remaining = $tokens - 1.0;
        $this->buckets[$key] = ['tokens' => $remaining, 'updatedAt' => $now];
        return new RateLimitDecision(true, $capacity, (int) floor($remaining));
    }
}

