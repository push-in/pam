<?php

declare(strict_types=1);

namespace Pam\Api\Tests\RateLimit;

use Pam\Api\RateLimit\MemoryRateLimitStore;
use PHPUnit\Framework\TestCase;

final class MemoryRateLimitStoreTest extends TestCase
{
    public function testTokenBucketIsDeterministicAndRefills(): void
    {
        $store = new MemoryRateLimitStore();

        self::assertTrue($store->consume('user:1', 2, 2, 100.0)->allowed);
        self::assertTrue($store->consume('user:1', 2, 2, 100.0)->allowed);
        $limited = $store->consume('user:1', 2, 2, 100.0);
        self::assertFalse($limited->allowed);
        self::assertSame(1, $limited->retryAfterSeconds);

        self::assertTrue($store->consume('user:1', 2, 2, 100.5)->allowed);
    }

    public function testFullStoreFailsClosedUntilAnIdleBucketCanBeEvicted(): void
    {
        $store = new MemoryRateLimitStore(maxBuckets: 1, idleTtlSeconds: 10.0);
        $store->consume('first', 1, 1, 100.0);

        self::assertFalse($store->consume('second', 1, 1, 105.0)->allowed);
        self::assertTrue($store->consume('second', 1, 1, 111.0)->allowed);
    }
}

