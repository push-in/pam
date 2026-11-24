<?php

declare(strict_types=1);

namespace Nimbus\Scheduling;

use Nimbus\Cache\Lock;
use Nimbus\Cache\Repository;

/**
 * The scheduler lock over the application cache (`Repository::lock()`): the Redis store
 * shares it across servers, as `Cache::lock()` does for application code.
 */
final class CacheSchedulerLock implements SchedulerLock
{
    /** @var array<string, Lock> */
    private array $locks = [];

    public function __construct(private readonly Repository $cache, private readonly string $prefix = 'schedule:')
    {
    }

    public function acquire(string $key, int $seconds): bool
    {
        $lock = $this->cache->lock($this->prefix . $key, max(1, $seconds));
        if (!$lock->acquire()) {
            return false;
        }
        $this->locks[$key] = $lock;
        return true;
    }

    public function release(string $key): void
    {
        $lock = $this->locks[$key] ?? null;
        unset($this->locks[$key]);
        $lock?->release();
    }
}
