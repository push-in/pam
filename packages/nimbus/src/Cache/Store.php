<?php

declare(strict_types=1);

namespace Nimbus\Cache;

/**
 * A key-value cache driver. Every store supports `Repository::tags()`: tagged keys are
 * namespaced by per-tag versions kept in the store itself.
 */
interface Store
{
    /** The value, or null when the key is missing or expired. */
    public function get(string $key): mixed;

    /** @param ?int $seconds Time to live; null keeps the value until it is evicted or forgotten. */
    public function put(string $key, mixed $value, ?int $seconds = null): void;

    /** Stores the value only when the key is missing; true when it was stored. */
    public function add(string $key, mixed $value, ?int $seconds = null): bool;

    public function increment(string $key, int $by = 1): int;

    public function forget(string $key): void;

    /** Removes every key of this store (or of its prefix). */
    public function flush(): void;
}
