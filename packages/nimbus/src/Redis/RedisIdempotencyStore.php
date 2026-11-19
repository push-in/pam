<?php

declare(strict_types=1);

namespace Nimbus\Redis;

use Nimbus\Idempotency\IdempotencyRecord;
use Nimbus\Idempotency\IdempotencyStore;

final readonly class RedisIdempotencyStore implements IdempotencyStore
{
    public function __construct(
        private Connection $redis,
        private string $prefix = 'nimbus:idempotency:',
    ) {
    }

    public function get(string $key, int $now): ?IdempotencyRecord
    {
        $decoded = SnapshotCodec::decode($this->redis->command(['GET', $this->prefix . $key]));
        if ($decoded === null) {
            return null;
        }
        [$snapshot, $data] = $decoded;
        $expiresAt = $data['expiresAt'] ?? null;
        $fingerprint = $data['fingerprint'] ?? null;
        if (!is_int($expiresAt) || !is_string($fingerprint) || $expiresAt <= $now) {
            return null;
        }
        return new IdempotencyRecord($fingerprint, $snapshot, $expiresAt);
    }

    public function put(string $key, IdempotencyRecord $record): void
    {
        $this->redis->command([
            'SET',
            $this->prefix . $key,
            SnapshotCodec::encode($record->response, ['fingerprint' => $record->fingerprint, 'expiresAt' => $record->expiresAt]),
            'EXAT',
            $record->expiresAt,
        ]);
    }
}
