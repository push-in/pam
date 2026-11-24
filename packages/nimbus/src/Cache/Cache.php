<?php

declare(strict_types=1);

namespace Nimbus\Cache;

/**
 * Static access to the worker's default cache repository (`Cache::useRepository()`, or
 * an in-memory store when none was set). Prefer injecting `Repository` in services.
 *
 * ```php
 * Cache::tags(['users', 'user:' . $id])->put('profile:' . $id, $profile, 600);
 * Cache::tags(['user:' . $id])->flush();
 * ```
 */
final class Cache
{
    private static ?Repository $repository = null;

    private function __construct()
    {
    }

    public static function useRepository(?Repository $repository): void
    {
        self::$repository = $repository;
    }

    public static function repository(): Repository
    {
        return self::$repository ??= new Repository(new MemoryStore());
    }

    public static function get(string $key, mixed $default = null): mixed
    {
        return self::repository()->get($key, $default);
    }

    public static function has(string $key): bool
    {
        return self::repository()->has($key);
    }

    public static function put(string $key, mixed $value, int|\DateTimeInterface|null $ttl = null): void
    {
        self::repository()->put($key, $value, $ttl);
    }

    public static function forever(string $key, mixed $value): void
    {
        self::repository()->forever($key, $value);
    }

    public static function add(string $key, mixed $value, int|\DateTimeInterface|null $ttl = null): bool
    {
        return self::repository()->add($key, $value, $ttl);
    }

    /**
     * @template T
     * @param \Closure(): T $callback
     * @return T
     */
    public static function remember(string $key, int|\DateTimeInterface|null $ttl, \Closure $callback): mixed
    {
        return self::repository()->remember($key, $ttl, $callback);
    }

    public static function pull(string $key, mixed $default = null): mixed
    {
        return self::repository()->pull($key, $default);
    }

    public static function increment(string $key, int $by = 1): int
    {
        return self::repository()->increment($key, $by);
    }

    public static function forget(string $key): void
    {
        self::repository()->forget($key);
    }

    public static function flush(): void
    {
        self::repository()->flush();
    }

    /** @param string|list<string> $tags */
    public static function tags(string|array $tags): TaggedCache
    {
        return self::repository()->tags($tags);
    }
}
