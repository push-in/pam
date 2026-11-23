<?php

declare(strict_types=1);

namespace Nimbus\Tests\RateLimit;

use Nimbus\RateLimit\SharedTableRateLimitStore;
use Pam\Shared\Table;
use PHPUnit\Framework\Attributes\CoversClass;
use PHPUnit\Framework\TestCase;

#[CoversClass(SharedTableRateLimitStore::class)]
final class SharedTableRateLimitStoreTest extends TestCase
{
    public function testWorkersSpendTheSameBucket(): void
    {
        $table = new Table(16, 0.2, 'limits');
        // Two workers open the same named table.
        $first = new SharedTableRateLimitStore($table, 16);
        $second = new SharedTableRateLimitStore($table, 16);

        self::assertTrue($first->consume('auth:1.2.3.4', 1, 2, 100.0)->allowed);
        self::assertTrue($second->consume('auth:1.2.3.4', 1, 2, 100.0)->allowed);
        $refused = $first->consume('auth:1.2.3.4', 1, 2, 100.0);
        self::assertFalse($refused->allowed);
        self::assertSame(1, $refused->retryAfterSeconds);
        self::assertTrue($second->consume('auth:1.2.3.4', 1, 2, 101.0)->allowed, 'Tokens refill over time.');
        self::assertTrue($second->consume('auth:5.6.7.8', 1, 2, 101.0)->allowed, 'Keys are independent.');
    }

    public function testAFullTableSweepsIdleBucketsAndRefusesWhenItCannot(): void
    {
        $table = new Table(2, 0.2, 'limits');
        $store = new SharedTableRateLimitStore($table, 2, idleTtlSeconds: 10.0);

        self::assertTrue($store->consume('a', 1, 5, 100.0)->allowed);
        self::assertTrue($store->consume('b', 1, 5, 100.0)->allowed);
        self::assertFalse($store->consume('c', 1, 5, 105.0)->allowed, 'No idle bucket to drop yet.');
        self::assertTrue($store->consume('c', 1, 5, 120.0)->allowed, 'Idle buckets were swept.');
        self::assertSame(1, $table->count());
    }

    public function testLongKeysFitTheTable(): void
    {
        $store = new SharedTableRateLimitStore(new Table(4, 0.2, 'limits'), 4);
        $key = str_repeat('k', 300);
        self::assertTrue($store->consume($key, 1, 1, 1.0)->allowed);
        self::assertFalse($store->consume($key, 1, 1, 1.0)->allowed);
    }
}
