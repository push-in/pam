<?php

declare(strict_types=1);

namespace Nimbus\Scheduling;

/**
 * A lock inside one PHP process: enough for tests and for a single scheduler
 * worker (`--task-workers 1`). Use RedisSchedulerLock or DatabaseSchedulerLock
 * when more than one worker or server ticks.
 */
final class MemorySchedulerLock implements SchedulerLock
{
    /** @var array<string, int> Expiry of each held key. */
    private array $held = [];

    /** @var \Closure(): int */
    private readonly \Closure $clock;

    /** @param (\Closure(): int)|null $clock Seconds since the epoch; defaults to time(). */
    public function __construct(?\Closure $clock = null)
    {
        $this->clock = $clock ?? time(...);
    }

    public function acquire(string $key, int $seconds): bool
    {
        $now = ($this->clock)();
        foreach ($this->held as $heldKey => $expiresAt) {
            if ($expiresAt <= $now) {
                unset($this->held[$heldKey]);
            }
        }
        if (isset($this->held[$key])) {
            return false;
        }
        $this->held[$key] = $now + max(1, $seconds);
        return true;
    }

    public function release(string $key): void
    {
        unset($this->held[$key]);
    }
}
