<?php

declare(strict_types=1);

namespace Nimbus\Scheduling;

use Nimbus\Redis\Connection;
use Nimbus\Redis\RedisLock;

/** A lock shared by every server through Redis (`RedisLock`: `SET NX EX`, released by owner only). */
final class RedisSchedulerLock implements SchedulerLock
{
    /** @var array<string, RedisLock> The lock of each key this instance holds. */
    private array $locks = [];

    public function __construct(
        private readonly Connection $redis,
        private readonly string $prefix = 'nimbus:lock:',
    ) {
    }

    public function acquire(string $key, int $seconds): bool
    {
        $lock = new RedisLock($this->redis, $key, max(1, $seconds), null, $this->prefix);
        if (!$lock->acquire()) {
            return false;
        }
        $this->locks[$key] = $lock;
        return true;
    }

    public function release(string $key): void
    {
        $lock = $this->locks[$key] ?? null;
        if ($lock === null) {
            return;
        }
        unset($this->locks[$key]);
        $lock->release();
    }
}
