<?php

declare(strict_types=1);

namespace Nimbus\Redis;

use Nimbus\Auth\OAuth\OAuthStateCodec;
use Nimbus\Auth\OAuth\OAuthStateStore;

/** Pending OAuth authorizations in Redis, read once with GETDEL (Redis 6.2+). */
final readonly class RedisOAuthStateStore implements OAuthStateStore
{
    public function __construct(
        private Connection $redis,
        private string $prefix = 'nimbus:oauth:',
    ) {
    }

    public function put(string $state, array $data, int $expiresAt): void
    {
        $this->redis->command([
            'SET',
            $this->prefix . OAuthStateCodec::key($state),
            OAuthStateCodec::encode($data, $expiresAt),
            'EXAT',
            $expiresAt,
        ]);
    }

    public function pull(string $state, int $now): ?array
    {
        return OAuthStateCodec::decode(
            $this->redis->command(['GETDEL', $this->prefix . OAuthStateCodec::key($state)]),
            $now,
        );
    }
}
