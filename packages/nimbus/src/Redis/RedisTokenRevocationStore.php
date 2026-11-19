<?php

declare(strict_types=1);

namespace Nimbus\Redis;

use Nimbus\Auth\TokenRevocationStore;

/** Revoked access-token identifiers shared through Redis; each key expires with its token. */
final readonly class RedisTokenRevocationStore implements TokenRevocationStore
{
    public function __construct(
        private Connection $redis,
        private string $prefix = 'nimbus:revoked:',
    ) {
    }

    public function revoke(string $tokenIdentifier, int $expiresAt): void
    {
        if ($tokenIdentifier === '' || $expiresAt < 1) {
            throw new \InvalidArgumentException('Revoked token identifier and expiry are required.');
        }
        $this->redis->command(['SET', $this->prefix . $tokenIdentifier, '1', 'EXAT', $expiresAt]);
    }

    public function isRevoked(string $tokenIdentifier, int $now): bool
    {
        return $this->redis->command(['GET', $this->prefix . $tokenIdentifier]) !== null;
    }
}
