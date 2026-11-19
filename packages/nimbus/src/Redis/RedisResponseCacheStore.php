<?php

declare(strict_types=1);

namespace Nimbus\Redis;

use Nimbus\Cache\CacheRecord;
use Nimbus\Cache\ResponseCacheStore;

final readonly class RedisResponseCacheStore implements ResponseCacheStore
{
    public function __construct(
        private Connection $redis,
        private string $prefix = 'nimbus:response:',
    ) {
    }

    public function get(string $key, int $now): ?CacheRecord
    {
        $decoded = SnapshotCodec::decode($this->redis->command(['GET', $this->prefix . $key]));
        if ($decoded === null) {
            return null;
        }
        [$snapshot, $data] = $decoded;
        $expiresAt = $data['expiresAt'] ?? null;
        return is_int($expiresAt) && $expiresAt > $now ? new CacheRecord($snapshot, $expiresAt) : null;
    }

    public function put(string $key, CacheRecord $record): void
    {
        $this->redis->command([
            'SET',
            $this->prefix . $key,
            SnapshotCodec::encode($record->response, ['expiresAt' => $record->expiresAt]),
            'EXAT',
            $record->expiresAt,
        ]);
    }

    public function forget(string $key): void
    {
        $this->redis->command(['DEL', $this->prefix . $key]);
    }
}
