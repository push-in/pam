<?php

declare(strict_types=1);

namespace Pam\Api\Cache;

final class MemoryResponseCacheStore implements ResponseCacheStore
{
    /** @var array<string, CacheRecord> */
    private array $records = [];

    public function __construct(private readonly int $maximumRecords = 10_000)
    {
        if ($maximumRecords < 1) {
            throw new \InvalidArgumentException('Maximum cache records must be positive.');
        }
    }

    public function get(string $key, int $now): ?CacheRecord
    {
        $record = $this->records[$key] ?? null;
        if ($record !== null && $record->expiresAt <= $now) {
            unset($this->records[$key]);
            return null;
        }
        return $record;
    }

    public function put(string $key, CacheRecord $record): void
    {
        if (!isset($this->records[$key]) && count($this->records) >= $this->maximumRecords) {
            throw new \RuntimeException('The bounded response cache is full.');
        }
        $this->records[$key] = $record;
    }

    public function forget(string $key): void
    {
        unset($this->records[$key]);
    }
}

