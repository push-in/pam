<?php

declare(strict_types=1);

namespace Nimbus\Redis;

use Pam\Redis\Client;

/**
 * Hands each command an idle connection, so request Fibers never share a socket
 * mid-command; at most `$maxIdle` connections stay open between requests.
 */
final class RedisPool implements Connection
{
    /** @var list<Connection> */
    private array $idle = [];

    /** @param \Closure(): Connection $connect */
    public function __construct(
        private readonly \Closure $connect,
        private readonly int $maxIdle = 16,
    ) {
        if ($maxIdle < 1) {
            throw new \InvalidArgumentException('A Redis pool keeps at least one idle connection.');
        }
    }

    /** Reads REDIS_HOST, REDIS_PORT, REDIS_PASSWORD and REDIS_DB. */
    public static function fromEnvironment(int $maxIdle = 16): self
    {
        $environment = static function (string $key, string $default): string {
            $value = getenv($key);
            return is_string($value) && $value !== '' ? $value : $default;
        };
        $host = $environment('REDIS_HOST', '127.0.0.1');
        $port = (int) $environment('REDIS_PORT', '6379');
        $password = $environment('REDIS_PASSWORD', '');
        $database = (int) $environment('REDIS_DB', '0');
        return new self(
            static fn (): Connection => new PamConnection(new Client(
                $host,
                $port,
                password: $password === '' ? null : $password,
                database: $database,
            )),
            $maxIdle,
        );
    }

    public function command(array $arguments): mixed
    {
        $connection = array_pop($this->idle) ?? ($this->connect)();
        // A failed connection is dropped; the next command opens a fresh one.
        $reply = $connection->command($arguments);
        if (count($this->idle) < $this->maxIdle) {
            $this->idle[] = $connection;
        }
        return $reply;
    }
}
