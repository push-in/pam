<?php

declare(strict_types=1);

namespace Nimbus\Tests\Cache;

use Nimbus\Cache\Cache;
use Nimbus\Cache\MemoryStore;
use Nimbus\Cache\Repository;
use Nimbus\Cache\Store;
use Nimbus\Cache\TaggedCache;
use Nimbus\Cache\TagSet;
use Nimbus\Redis\RedisCacheStore;
use Nimbus\Tests\Redis\SocketConnection;
use PHPUnit\Framework\Attributes\CoversClass;
use PHPUnit\Framework\Attributes\DataProvider;
use PHPUnit\Framework\TestCase;

#[CoversClass(Repository::class)]
#[CoversClass(TaggedCache::class)]
#[CoversClass(TagSet::class)]
#[CoversClass(MemoryStore::class)]
#[CoversClass(RedisCacheStore::class)]
#[CoversClass(Cache::class)]
final class CacheTest extends TestCase
{
    protected function tearDown(): void
    {
        Cache::useRepository(null);
    }

    /** @return iterable<string, array{\Closure(): Store}> */
    public static function stores(): iterable
    {
        yield 'memory' => [static fn (): Store => new MemoryStore()];
        yield 'redis (emulated)' => [static fn (): Store => new RedisCacheStore(new FakeRedis(), 'test:')];
        $address = getenv('NIMBUS_TEST_REDIS');
        if (is_string($address) && $address !== '') {
            yield 'redis' => [static fn (): Store => new RedisCacheStore(new SocketConnection($address), 'nimbus-test:' . bin2hex(random_bytes(4)) . ':')];
        }
    }

    /** @param \Closure(): Store $store */
    #[DataProvider('stores')]
    public function testItStoresRemembersAndCounts(\Closure $store): void
    {
        $cache = new Repository($store());

        $cache->put('a', ['x' => 1], 60);
        self::assertSame(['x' => 1], $cache->get('a'));
        self::assertTrue($cache->has('a'));
        self::assertSame('d', $cache->get('missing', 'd'));
        self::assertSame('lazy', $cache->get('missing', static fn (): string => 'lazy'));
        $calls = 0;
        $compute = static function () use (&$calls): string {
            ++$calls;
            return 'computed';
        };
        self::assertSame('computed', $cache->remember('r', 60, $compute));
        $cache->remember('r', 60, $compute);
        self::assertSame(1, $calls);
        self::assertTrue($cache->add('once', 1));
        self::assertFalse($cache->add('once', 2));
        self::assertSame(3, $cache->increment('n', 3));
        self::assertSame(1, $cache->decrement('n', 2));
        self::assertSame(1, $cache->get('n'));
        self::assertSame(['x' => 1], $cache->pull('a'));
        self::assertFalse($cache->has('a'));
        $cache->put('gone', 'x', 0);
        self::assertNull($cache->get('gone'));
        $cache->flush();
        self::assertNull($cache->get('r'));
    }

    /** @param \Closure(): Store $store */
    #[DataProvider('stores')]
    public function testTagsFlushOnlyTheirKeys(\Closure $store): void
    {
        $cache = new Repository($store());
        $cache->put('plain', 'p');
        $cache->tags(['products'])->put('page:1', 'products');
        $cache->tags(['products', 'users'])->put('mixed', 'both');
        $cache->tags('users')->put('profile', 'user');

        self::assertSame('products', $cache->tags('products')->get('page:1'));
        self::assertNull($cache->get('page:1'), 'Tagged keys live in their own namespace.');
        self::assertSame('both', $cache->tags(['products'])->tags('users')->get('mixed'));

        $cache->tags('products')->flush();

        self::assertNull($cache->tags('products')->get('page:1'));
        self::assertNull($cache->tags(['products', 'users'])->get('mixed'));
        self::assertSame('user', $cache->tags('users')->get('profile'));
        self::assertSame('p', $cache->get('plain'));
        self::assertSame('again', $cache->tags('products')->remember('page:1', 60, static fn (): string => 'again'));
    }

    public function testMemoryStoreExpiresAndEvicts(): void
    {
        $now = 1_000;
        $store = new MemoryStore(2, static function () use (&$now): int {
            return $now;
        });
        $store->put('a', 1, 10);
        $store->put('b', 2);
        $store->put('c', 3);
        self::assertNull($store->get('a'), 'The oldest entry is evicted.');
        $store->put('d', 4, 5);
        $now += 5;
        self::assertNull($store->get('d'));
        self::assertSame(3, $store->get('c'));
    }

    public function testRedisStoreRefusesObjectsByDefault(): void
    {
        $store = new RedisCacheStore(new FakeRedis());
        $store->put('o', new \ArrayObject([1]));
        self::assertInstanceOf(\__PHP_Incomplete_Class::class, $store->get('o'));

        $allowed = new RedisCacheStore(new FakeRedis(), allowedClasses: [\ArrayObject::class]);
        $allowed->put('o', new \ArrayObject([1]));
        self::assertEquals(new \ArrayObject([1]), $allowed->get('o'));
        $allowed->put('f', false);
        self::assertFalse($allowed->get('f'));

        $this->expectException(\LogicException::class);
        (new RedisCacheStore(new FakeRedis(), ''))->flush();
    }

    public function testTheFacadeUsesTheDefaultRepository(): void
    {
        $repository = new Repository(new MemoryStore());
        Cache::useRepository($repository);

        Cache::tags(['a'])->put('k', 'v', 60);
        Cache::put('plain', 1);
        self::assertSame('v', $repository->tags('a')->get('k'));
        self::assertSame(1, Cache::get('plain'));
        Cache::tags('a')->flush();
        self::assertNull(Cache::tags('a')->get('k'));
        self::assertSame(2, Cache::increment('plain'));
        Cache::forget('plain');
        self::assertFalse(Cache::has('plain'));
    }
}
