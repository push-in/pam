<?php

declare(strict_types=1);

namespace Nimbus\Redis;

use Nimbus\Session\SessionPayload;
use Nimbus\Session\SessionStore;

/** Sessions as Redis strings that expire with the session lifetime. */
final readonly class RedisSessionStore implements SessionStore
{
    public function __construct(
        private Connection $redis,
        private string $prefix = 'nimbus:session:',
    ) {
    }

    public function read(string $id): ?array
    {
        return SessionPayload::validId($id) ? SessionPayload::decode($this->redis->command(['GET', $this->prefix . $id])) : null;
    }

    public function write(string $id, array $data, int $lifetimeSeconds): void
    {
        if (!SessionPayload::validId($id) || $lifetimeSeconds < 1) {
            throw new \InvalidArgumentException('A session needs a valid id and a positive lifetime.');
        }
        $this->redis->command(['SET', $this->prefix . $id, SessionPayload::encode($data), 'EX', $lifetimeSeconds]);
    }

    public function destroy(string $id): void
    {
        if (SessionPayload::validId($id)) {
            $this->redis->command(['DEL', $this->prefix . $id]);
        }
    }

    public function gc(): int
    {
        return 0;
    }
}
