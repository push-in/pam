<?php

declare(strict_types=1);

namespace Nimbus\Redis;

use Nimbus\RateLimit\RateLimitDecision;
use Nimbus\RateLimit\RateLimitStore;

/** Token buckets shared by every worker and server, updated atomically by a Lua script. */
final readonly class RedisRateLimitStore implements RateLimitStore
{
    private const string SCRIPT = <<<'LUA'
        local bucket = redis.call('HMGET', KEYS[1], 't', 'u')
        local rate, capacity, now = tonumber(ARGV[1]), tonumber(ARGV[2]), tonumber(ARGV[3])
        local tokens = tonumber(bucket[1]) or capacity
        local updated = tonumber(bucket[2]) or now
        tokens = math.min(capacity, tokens + math.max(0, now - updated) * rate)
        local allowed = 0
        if tokens >= 1 then
            tokens = tokens - 1
            allowed = 1
        end
        redis.call('HSET', KEYS[1], 't', tostring(tokens), 'u', tostring(now))
        redis.call('EXPIRE', KEYS[1], math.ceil(capacity / rate) + 1)
        return {allowed, tostring(tokens)}
        LUA;

    public function __construct(
        private Connection $redis,
        private string $prefix = 'nimbus:rate:',
    ) {
    }

    public function consume(string $key, int $requestsPerSecond, int $capacity, float $now): RateLimitDecision
    {
        $arguments = [1, $this->prefix . $key, $requestsPerSecond, $capacity, sprintf('%.6F', $now)];
        try {
            $reply = $this->redis->command(['EVALSHA', sha1(self::SCRIPT), ...$arguments]);
        } catch (\Throwable $error) {
            if (!str_contains($error->getMessage(), 'NOSCRIPT')) {
                throw $error;
            }
            $reply = $this->redis->command(['EVAL', self::SCRIPT, ...$arguments]);
        }
        if (!is_array($reply) || count($reply) !== 2 || !is_numeric($reply[0]) || !is_numeric($reply[1])) {
            throw new \UnexpectedValueException('Unexpected Redis rate-limit reply.');
        }
        $tokens = (float) $reply[1];
        if ((int) $reply[0] === 1) {
            return new RateLimitDecision(true, $capacity, (int) floor($tokens));
        }
        return new RateLimitDecision(false, $capacity, 0, max(1, (int) ceil((1.0 - $tokens) / $requestsPerSecond)));
    }
}
