<?php

declare(strict_types=1);

namespace Nimbus\Tests\Cache;

use Nimbus\Cache\Cache;
use Nimbus\Cache\CacheLock;
use Nimbus\Cache\Lock;
use Nimbus\Cache\LockTimeoutException;
use Nimbus\Cache\MemoryStore;
use Nimbus\Cache\Repository;
use Nimbus\Redis\RedisCacheStore;
use Nimbus\Redis\RedisLock;
use Nimbus\Scheduling\CacheSchedulerLock;
use PHPUnit\Framework\Attributes\CoversClass;
use PHPUnit\Framework\Attributes\DataProvider;
use PHPUnit\Framework\TestCase;

#[CoversClass(Lock::class)]
#[CoversClass(CacheLock::class)]
#[CoversClass(RedisLock::class)]
#[CoversClass(CacheSchedulerLock::class)]
final class CacheLockTest extends TestCase
{
    protected function tearDown(): void
    {
        Cache::useRepository(null);
    }

    /** @return iterable<string, array{Repository}> */
    public static function repositories(): iterable
    {
        yield 'memory' => [new Repository(new MemoryStore())];
        yield 'redis' => [new Repository(new RedisCacheStore(new FakeRedis()))];
    }

    #[DataProvider('repositories')]
    public function testOnlyOneOwnerHoldsTheLock(Repository $cache): void
    {
        $first = $cache->lock('ask:7', 30);
        $second = $cache->lock('ask:7', 30);

        self::assertTrue($first->get());
        self::assertFalse($second->get());
        self::assertTrue($first->isOwnedByCurrentProcess());
        self::assertFalse($second->isOwnedByCurrentProcess());
        self::assertFalse($second->release());
        self::assertTrue($first->release());
        self::assertTrue($second->get());
        $second->forceRelease();
        self::assertTrue($cache->lock('ask:7')->get());
    }

    #[DataProvider('repositories')]
    public function testGetRunsTheCallbackUnderTheLockAndReleasesIt(Repository $cache): void
    {
        $result = $cache->lock('report', 10)->get(static function () use ($cache): string {
            self::assertFalse($cache->lock('report')->get(), 'Held while the callback runs.');
            return 'done';
        });

        self::assertSame('done', $result);
        self::assertTrue($cache->lock('report')->get());
        self::assertFalse($cache->lock('report')->get(static fn (): string => 'never'));
    }

    #[DataProvider('repositories')]
    public function testOwnersRestoreTheLockInAnotherProcess(Repository $cache): void
    {
        $lock = $cache->lock('job', 60);
        self::assertTrue($lock->get());

        $restored = $cache->restoreLock('job', $lock->owner());

        self::assertTrue($restored->isOwnedByCurrentProcess());
        self::assertTrue($restored->release());
        self::assertTrue($cache->lock('job')->get());
    }

    public function testBlockWaitsAndThenGivesUp(): void
    {
        $cache = new Repository(new MemoryStore());
        $held = $cache->lock('heartbeat', 30);
        $held->get();

        $started = microtime(true);
        try {
            $cache->lock('heartbeat')->betweenBlockedAttemptsSleepFor(10)->block(0);
            self::fail('The lock is held.');
        } catch (LockTimeoutException $error) {
            self::assertStringContainsString('heartbeat', $error->getMessage());
        }
        self::assertLessThan(1.0, microtime(true) - $started);

        $held->release();
        self::assertSame('beat', $cache->lock('heartbeat')->block(1, static fn (): string => 'beat'));
        $cache->lock('heartbeat')->block(1);
        self::assertFalse($cache->lock('heartbeat')->get(), 'block() without a callback keeps the lock.');
    }

    public function testTheFacadeAndTheSchedulerUseTheSameLocks(): void
    {
        $cache = new Repository(new MemoryStore());
        Cache::useRepository($cache);
        self::assertTrue(Cache::lock('schedule:nightly', 60)->get());

        $scheduler = new CacheSchedulerLock($cache);
        self::assertFalse($scheduler->acquire('nightly', 60));
        Cache::restoreLock('schedule:nightly', 'someone-else')->forceRelease();
        self::assertTrue($scheduler->acquire('nightly', 60));
        self::assertFalse((new CacheSchedulerLock($cache))->acquire('nightly', 60));
        $scheduler->release('nightly');
        self::assertTrue((new CacheSchedulerLock($cache))->acquire('nightly', 60));
    }

    public function testInvalidLocksAreRejected(): void
    {
        $this->expectException(\InvalidArgumentException::class);
        (new Repository(new MemoryStore()))->lock('', 1);
    }
}
