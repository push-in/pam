<?php

declare(strict_types=1);

namespace Pam\Api\Idempotency;

final class MemoryIdempotencyStore implements IdempotencyStore
{
    /** @var array<string, IdempotencyRecord> */
    private array $records = [];

    public function __construct(private readonly int $maximumRecords = 10_000)
    {
        if ($maximumRecords < 1) {
            throw new \InvalidArgumentException('Maximum idempotency records must be positive.');
        }
    }

    public function get(string $key, int $now): ?IdempotencyRecord
    {
        $record = $this->records[$key] ?? null;
        if ($record !== null && $record->expiresAt <= $now) {
            unset($this->records[$key]);
            return null;
        }
        return $record;
    }

    public function put(string $key, IdempotencyRecord $record): void
    {
        if (!isset($this->records[$key]) && count($this->records) >= $this->maximumRecords) {
            throw new \RuntimeException('The bounded idempotency store is full.');
        }
        $this->records[$key] = $record;
    }
}

