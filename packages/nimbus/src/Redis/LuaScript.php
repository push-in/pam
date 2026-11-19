<?php

declare(strict_types=1);

namespace Nimbus\Redis;

/**
 * A Lua script sent by digest, loaded on the first NOSCRIPT reply.
 *
 * @internal
 */
final readonly class LuaScript
{
    private string $digest;

    public function __construct(private string $source)
    {
        $this->digest = sha1($source);
    }

    /**
     * @param list<string> $keys
     * @param list<string|int|float> $arguments
     */
    public function run(Connection $redis, array $keys, array $arguments = []): mixed
    {
        $tail = [count($keys), ...$keys, ...$arguments];
        try {
            return $redis->command(['EVALSHA', $this->digest, ...$tail]);
        } catch (\Throwable $error) {
            if (!str_contains($error->getMessage(), 'NOSCRIPT')) {
                throw $error;
            }
            return $redis->command(['EVAL', $this->source, ...$tail]);
        }
    }
}
