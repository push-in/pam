<?php

declare(strict_types=1);

namespace Nimbus\Redis;

use Nimbus\Cache\Lock;
use Nimbus\Cache\LockProvider;
use Nimbus\Cache\Store;

/**
 * A cache store in Redis. Integers are stored as Redis integers (so `increment()` is
 * atomic); other values are PHP-serialized. Objects are restored only for the classes in
 * `$allowedClasses` (none by default), so a written cache entry cannot instantiate code.
 */
final readonly class RedisCacheStore implements Store, LockProvider
{
    /** @param bool|list<class-string> $allowedClasses Passed to unserialize(). */
    public function __construct(
        private Connection $redis,
        private string $prefix = 'nimbus:cache:',
        private bool|array $allowedClasses = false,
    ) {
    }

    public function get(string $key): mixed
    {
        $value = $this->redis->command(['GET', $this->prefix . $key]);
        if (!is_string($value)) {
            return is_int($value) ? $value : null;
        }
        if (preg_match('/^-?[0-9]+$/D', $value) === 1) {
            return (int) $value;
        }
        $decoded = @unserialize($value, ['allowed_classes' => $this->allowedClasses]);
        return $decoded === false && $value !== serialize(false) ? null : $decoded;
    }

    public function put(string $key, mixed $value, ?int $seconds = null): void
    {
        if ($seconds !== null && $seconds <= 0) {
            $this->forget($key);
            return;
        }
        $command = ['SET', $this->prefix . $key, self::encode($value)];
        if ($seconds !== null) {
            array_push($command, 'EX', $seconds);
        }
        $this->redis->command($command);
    }

    public function add(string $key, mixed $value, ?int $seconds = null): bool
    {
        $command = ['SET', $this->prefix . $key, self::encode($value), 'NX'];
        if ($seconds !== null) {
            array_push($command, 'EX', max(1, $seconds));
        }
        $reply = $this->redis->command($command);
        return $reply === true || $reply === 'OK';
    }

    public function increment(string $key, int $by = 1): int
    {
        $value = $this->redis->command(['INCRBY', $this->prefix . $key, $by]);
        return is_int($value) ? $value : (int) (is_string($value) ? $value : 0);
    }

    public function forget(string $key): void
    {
        $this->redis->command(['DEL', $this->prefix . $key]);
    }

    /** Deletes the keys of this store's prefix with SCAN, in batches. */
    public function flush(): void
    {
        if ($this->prefix === '') {
            throw new \LogicException('Refusing to flush a Redis cache store without a key prefix.');
        }
        $cursor = '0';
        do {
            $reply = $this->redis->command(['SCAN', $cursor, 'MATCH', $this->prefix . '*', 'COUNT', 500]);
            if (!is_array($reply) || !isset($reply[0], $reply[1]) || !is_array($reply[1])) {
                return;
            }
            $cursor = is_scalar($reply[0]) ? (string) $reply[0] : '0';
            $keys = array_values(array_filter($reply[1], is_string(...)));
            if ($keys !== []) {
                $this->redis->command(['UNLINK', ...$keys]);
            }
        } while ($cursor !== '0');
    }

    /** A `RedisLock` under this store's prefix (`{prefix}lock:{name}`). */
    public function lock(string $name, int $seconds = 0, ?string $owner = null): Lock
    {
        return new RedisLock($this->redis, $name, $seconds, $owner, $this->prefix . 'lock:');
    }

    public function restoreLock(string $name, string $owner): Lock
    {
        return $this->lock($name, 0, $owner);
    }

    private static function encode(mixed $value): string|int
    {
        return is_int($value) ? $value : serialize($value);
    }
}
