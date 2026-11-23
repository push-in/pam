<?php

declare(strict_types=1);

namespace Nimbus\Cache;

/**
 * A per-worker cache in PHP memory, bounded by entry count (oldest entries are evicted
 * first). Values are kept as they are, so objects come back as the same instance.
 */
final class MemoryStore implements Store
{
    /** @var array<string, array{mixed, ?int}> Value and expiry timestamp. */
    private array $entries = [];

    /** @param ?\Closure(): int $clock */
    public function __construct(
        private readonly int $maximumEntries = 10_000,
        private readonly ?\Closure $clock = null,
    ) {
        if ($maximumEntries < 1) {
            throw new \InvalidArgumentException('Maximum cache entries must be positive.');
        }
    }

    public function get(string $key): mixed
    {
        $entry = $this->entries[$key] ?? null;
        if ($entry === null) {
            return null;
        }
        if ($entry[1] !== null && $entry[1] <= $this->now()) {
            unset($this->entries[$key]);
            return null;
        }
        return $entry[0];
    }

    public function put(string $key, mixed $value, ?int $seconds = null): void
    {
        if ($seconds !== null && $seconds <= 0) {
            $this->forget($key);
            return;
        }
        unset($this->entries[$key]);
        $this->entries[$key] = [$value, $seconds === null ? null : $this->now() + $seconds];
        while (count($this->entries) > $this->maximumEntries) {
            unset($this->entries[array_key_first($this->entries)]);
        }
    }

    public function add(string $key, mixed $value, ?int $seconds = null): bool
    {
        if ($this->get($key) !== null) {
            return false;
        }
        $this->put($key, $value, $seconds);
        return true;
    }

    public function increment(string $key, int $by = 1): int
    {
        $current = $this->get($key);
        $value = (is_int($current) ? $current : 0) + $by;
        $this->entries[$key] = [$value, $this->entries[$key][1] ?? null];
        return $value;
    }

    public function forget(string $key): void
    {
        unset($this->entries[$key]);
    }

    public function flush(): void
    {
        $this->entries = [];
    }

    private function now(): int
    {
        return $this->clock !== null ? ($this->clock)() : time();
    }
}
