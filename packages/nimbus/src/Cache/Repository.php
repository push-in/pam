<?php

declare(strict_types=1);

namespace Nimbus\Cache;

/**
 * The application cache over a Store.
 *
 * ```php
 * $cache = new Repository(new RedisCacheStore($redis));
 * $products = $cache->tags(['products'])->remember('page:1', 300, fn () => $repository->page(1));
 * $cache->tags(['products'])->flush(); // after a product changes
 * ```
 */
class Repository
{
    public function __construct(protected readonly Store $store)
    {
    }

    public function store(): Store
    {
        return $this->store;
    }

    public function get(string $key, mixed $default = null): mixed
    {
        $value = $this->store->get($this->key($key));
        if ($value !== null) {
            return $value;
        }
        return $default instanceof \Closure ? $default() : $default;
    }

    public function has(string $key): bool
    {
        return $this->store->get($this->key($key)) !== null;
    }

    /** @param int|\DateTimeInterface|null $ttl Seconds or an expiry time; null stores forever. */
    public function put(string $key, mixed $value, int|\DateTimeInterface|null $ttl = null): void
    {
        $this->store->put($this->key($key), $value, self::seconds($ttl));
    }

    public function forever(string $key, mixed $value): void
    {
        $this->put($key, $value);
    }

    public function add(string $key, mixed $value, int|\DateTimeInterface|null $ttl = null): bool
    {
        return $this->store->add($this->key($key), $value, self::seconds($ttl));
    }

    /**
     * The cached value, or the callback's result stored for the ttl. A null result is not cached.
     *
     * @template T
     * @param \Closure(): T $callback
     * @return T
     */
    public function remember(string $key, int|\DateTimeInterface|null $ttl, \Closure $callback): mixed
    {
        $value = $this->store->get($this->key($key));
        if ($value !== null) {
            /** @var T $value */
            return $value;
        }
        $value = $callback();
        if ($value !== null) {
            $this->put($key, $value, $ttl);
        }
        return $value;
    }

    /**
     * @template T
     * @param \Closure(): T $callback
     * @return T
     */
    public function rememberForever(string $key, \Closure $callback): mixed
    {
        return $this->remember($key, null, $callback);
    }

    /** The value, removed from the cache. */
    public function pull(string $key, mixed $default = null): mixed
    {
        $value = $this->get($key, $default);
        $this->forget($key);
        return $value;
    }

    public function increment(string $key, int $by = 1): int
    {
        return $this->store->increment($this->key($key), $by);
    }

    public function decrement(string $key, int $by = 1): int
    {
        return $this->store->increment($this->key($key), -$by);
    }

    public function forget(string $key): void
    {
        $this->store->forget($this->key($key));
    }

    /** Clears the whole store. */
    public function flush(): void
    {
        $this->store->flush();
    }

    /**
     * A view of the cache whose keys belong to the tags; `flush()` on it invalidates every
     * key stored with any of those tags (as the same tag set) without touching the others.
     *
     * @param string|list<string> $tags
     */
    public function tags(string|array $tags): TaggedCache
    {
        $tags = is_string($tags) ? [$tags] : $tags;
        if ($tags === []) {
            throw new \InvalidArgumentException('Cache tags cannot be empty.');
        }
        return new TaggedCache($this->store, new TagSet($this->store, $tags));
    }

    /**
     * An atomic lock: `Cache::lock('name', 10)->get(fn () => ...)`. Stores that implement
     * `LockProvider` (Redis) use their own; others lock through `add()`.
     */
    public function lock(string $name, int $seconds = 0, ?string $owner = null): Lock
    {
        return $this->store instanceof LockProvider
            ? $this->store->lock($name, $seconds, $owner)
            : new CacheLock($this->store, $name, $seconds, $owner);
    }

    /** The lock held by `$owner` (`Lock::owner()`), to release it from another process. */
    public function restoreLock(string $name, string $owner): Lock
    {
        return $this->store instanceof LockProvider
            ? $this->store->restoreLock($name, $owner)
            : new CacheLock($this->store, $name, 0, $owner);
    }

    protected function key(string $key): string
    {
        if ($key === '') {
            throw new \InvalidArgumentException('Cache keys cannot be empty.');
        }
        return $key;
    }

    private static function seconds(int|\DateTimeInterface|null $ttl): ?int
    {
        if ($ttl instanceof \DateTimeInterface) {
            return max(0, $ttl->getTimestamp() - time());
        }
        return $ttl;
    }
}
