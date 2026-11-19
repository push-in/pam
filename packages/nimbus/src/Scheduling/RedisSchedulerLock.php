<?php

declare(strict_types=1);

namespace Nimbus\Scheduling;

use Nimbus\Redis\Connection;
use Nimbus\Redis\LuaScript;

/** A lock shared by every server through Redis (`SET NX EX`, released by owner only). */
final class RedisSchedulerLock implements SchedulerLock
{
    private const string RELEASE = <<<'LUA'
        if redis.call('GET', KEYS[1]) == ARGV[1] then
            return redis.call('DEL', KEYS[1])
        end
        return 0
        LUA;

    /** @var array<string, string> Token of each key this instance holds. */
    private array $tokens = [];

    private readonly LuaScript $release;

    public function __construct(
        private readonly Connection $redis,
        private readonly string $prefix = 'nimbus:lock:',
    ) {
        $this->release = new LuaScript(self::RELEASE);
    }

    public function acquire(string $key, int $seconds): bool
    {
        $token = bin2hex(random_bytes(16));
        $reply = $this->redis->command(['SET', $this->prefix . $key, $token, 'NX', 'EX', max(1, $seconds)]);
        if ($reply !== 'OK' && $reply !== true) {
            return false;
        }
        $this->tokens[$key] = $token;
        return true;
    }

    public function release(string $key): void
    {
        $token = $this->tokens[$key] ?? null;
        if ($token === null) {
            return;
        }
        unset($this->tokens[$key]);
        $this->release->run($this->redis, [$this->prefix . $key], [$token]);
    }
}
