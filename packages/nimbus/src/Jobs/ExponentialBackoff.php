<?php

declare(strict_types=1);

namespace Nimbus\Jobs;

/**
 * Doubles the delay after each failed attempt, up to a ceiling. With jitter the
 * delay is drawn between half and all of it ("equal jitter"), so jobs that failed
 * together do not all come back in the same second.
 */
final readonly class ExponentialBackoff implements RetryPolicy
{
    private const int MAXIMUM_DELAY_SECONDS = 604_800;

    /** @var \Closure(int, int): int */
    private \Closure $random;

    /** @param (\Closure(int, int): int)|null $random Returns an integer between both bounds, inclusive. */
    public function __construct(
        private int $baseSeconds = 5,
        private int $maximumSeconds = 3_600,
        private bool $jitter = true,
        ?\Closure $random = null,
    ) {
        if ($baseSeconds < 1 || $maximumSeconds < $baseSeconds || $maximumSeconds > self::MAXIMUM_DELAY_SECONDS) {
            throw new \InvalidArgumentException('Backoff base and maximum delays are invalid.');
        }
        $this->random = $random ?? random_int(...);
    }

    public function delaySeconds(int $attempt): int
    {
        $exponent = min(max(0, $attempt - 1), 30);
        $delay = (int) min($this->maximumSeconds, $this->baseSeconds * (2 ** $exponent));
        if (!$this->jitter) {
            return max(0, $delay);
        }
        return max(0, min($delay, ($this->random)(intdiv($delay, 2), $delay)));
    }
}
