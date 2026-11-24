<?php

declare(strict_types=1);

namespace Nimbus\Redis;

use Nimbus\Cache\Lock;

/** A lock shared by every server: `SET NX [EX]`, released by its owner through a Lua compare-and-delete. */
final class RedisLock extends Lock
{
    private const string RELEASE = <<<'LUA'
        if redis.call('GET', KEYS[1]) == ARGV[1] then
            return redis.call('DEL', KEYS[1])
        end
        return 0
        LUA;

    private static ?LuaScript $releaseScript = null;

    public function __construct(
        private readonly Connection $redis,
        string $name,
        int $seconds = 0,
        ?string $owner = null,
        private readonly string $prefix = 'nimbus:lock:',
    ) {
        parent::__construct($name, $seconds, $owner);
    }

    public function acquire(): bool
    {
        $command = ['SET', $this->prefix . $this->name, $this->owner, 'NX'];
        if ($this->seconds > 0) {
            array_push($command, 'EX', $this->seconds);
        }
        $reply = $this->redis->command($command);
        return $reply === 'OK' || $reply === true;
    }

    public function release(): bool
    {
        self::$releaseScript ??= new LuaScript(self::RELEASE);
        $deleted = self::$releaseScript->run($this->redis, [$this->prefix . $this->name], [$this->owner]);
        return (int) (is_scalar($deleted) ? $deleted : 0) === 1;
    }

    public function forceRelease(): void
    {
        $this->redis->command(['DEL', $this->prefix . $this->name]);
    }

    protected function currentOwner(): ?string
    {
        $owner = $this->redis->command(['GET', $this->prefix . $this->name]);
        return is_string($owner) ? $owner : null;
    }
}
