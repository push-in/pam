<?php

declare(strict_types=1);

namespace Nimbus\Cache;

/** A store with its own atomic lock implementation (the Redis store). */
interface LockProvider
{
    public function lock(string $name, int $seconds = 0, ?string $owner = null): Lock;

    /** The lock of `$owner`, to release it from another process. */
    public function restoreLock(string $name, string $owner): Lock;
}
