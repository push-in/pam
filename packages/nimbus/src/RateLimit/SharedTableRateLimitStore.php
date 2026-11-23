<?php

declare(strict_types=1);

namespace Nimbus\RateLimit;

use Pam\Shared\Table;

/**
 * Token buckets in a named PAM shared table (`Pam\Shared\Table`), so every worker of the
 * cluster spends the same budget and the buckets survive worker recycling. The table has a
 * fixed number of rows: when it is full, idle buckets are swept, and a new key that still finds
 * no room is refused, like MemoryRateLimitStore.
 *
 * Two workers may read the same bucket at once and both spend from it; the overshoot is at most
 * one request per concurrent worker, which is what a throttle needs. Use RedisRateLimitStore to
 * share a limit across machines.
 */
final class SharedTableRateLimitStore implements RateLimitStore
{
    public const string DEFAULT_TABLE = 'nimbus_rate_limit';

    private const int KEY_BYTES = 64;

    private readonly Table $table;

    private int $consumed = 0;

    public function __construct(
        ?Table $table = null,
        private readonly int $rows = 65_536,
        private readonly float $idleTtlSeconds = 300.0,
        string $name = self::DEFAULT_TABLE,
    ) {
        if ($rows < 1 || $idleTtlSeconds <= 0) {
            throw new \InvalidArgumentException('Shared rate-limit store configuration is invalid.');
        }
        if ($table === null) {
            $table = new Table($rows, 0.2, $name, self::KEY_BYTES);
            $table->column('tokens', Table::TYPE_FLOAT);
            $table->column('updated_at', Table::TYPE_FLOAT);
            $table->create();
        }
        $this->table = $table;
    }

    /** Whether the running PAM runtime offers named shared tables. */
    public static function available(): bool
    {
        return class_exists(Table::class) && Table::available();
    }

    public function consume(
        string $key,
        int $requestsPerSecond,
        int $capacity,
        float $now,
    ): RateLimitDecision {
        // Keys of any length fit the table's fixed key size.
        $key = strlen($key) <= 48 ? $key : '#' . substr(hash('sha256', $key), 0, 40);
        $bucket = $this->table->get($key);
        if (!is_array($bucket)) {
            $bucket = ['tokens' => (float) $capacity, 'updated_at' => $now];
        }
        $elapsed = max(0.0, $now - (float) ($bucket['updated_at'] ?? $now));
        $tokens = min((float) $capacity, (float) ($bucket['tokens'] ?? $capacity) + ($elapsed * $requestsPerSecond));
        $allowed = $tokens >= 1.0;
        $remaining = $allowed ? $tokens - 1.0 : $tokens;
        if (!$this->store($key, $remaining, $now)) {
            return new RateLimitDecision(false, $capacity, 0, 1);
        }
        if (!$allowed) {
            return new RateLimitDecision(false, $capacity, 0, max(1, (int) ceil((1.0 - $tokens) / $requestsPerSecond)));
        }
        return new RateLimitDecision(true, $capacity, (int) floor($remaining));
    }

    private function store(string $key, float $tokens, float $now): bool
    {
        $values = ['tokens' => $tokens, 'updated_at' => $now];
        if ($this->table->set($key, $values)) {
            if ((++$this->consumed & 4095) === 0 && $this->table->count() > intdiv($this->rows * 3, 4)) {
                $this->sweep($now);
            }
            return true;
        }
        // Full table: drop idle buckets and try once more.
        $this->sweep($now);
        return $this->table->set($key, $values);
    }

    private function sweep(float $now): void
    {
        $threshold = $now - $this->idleTtlSeconds;
        $idle = [];
        foreach ($this->table as $key => $row) {
            if ((float) ($row['updated_at'] ?? 0.0) < $threshold) {
                $idle[] = (string) $key;
            }
        }
        foreach ($idle as $key) {
            $this->table->del($key);
        }
    }
}
