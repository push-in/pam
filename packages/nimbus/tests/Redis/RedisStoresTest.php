<?php

declare(strict_types=1);

namespace Nimbus\Tests\Redis;

use Nimbus\Cache\CacheRecord;
use Nimbus\Http\ResponseSnapshot;
use Nimbus\Idempotency\IdempotencyRecord;
use Nimbus\Redis\Connection;
use Nimbus\Redis\RedisIdempotencyStore;
use Nimbus\Redis\RedisPool;
use Nimbus\Redis\RedisRateLimitStore;
use Nimbus\Redis\RedisResponseCacheStore;
use PHPUnit\Framework\Attributes\CoversClass;
use PHPUnit\Framework\TestCase;

#[CoversClass(RedisPool::class)]
#[CoversClass(RedisIdempotencyStore::class)]
#[CoversClass(RedisResponseCacheStore::class)]
#[CoversClass(RedisRateLimitStore::class)]
final class RedisStoresTest extends TestCase
{
    public function testIdempotencyAndCacheRecordsRoundTrip(): void
    {
        $redis = new MemoryConnection();
        $snapshot = new ResponseSnapshot(201, ['content-type' => ['application/json']], "{\"id\":1}\0binary");

        $idempotency = new RedisIdempotencyStore($redis);
        $idempotency->put('k', new IdempotencyRecord('fp', $snapshot, 2_000));
        $record = $idempotency->get('k', 1_000);
        self::assertNotNull($record);
        self::assertSame('fp', $record->fingerprint);
        self::assertEquals($snapshot, $record->response);
        self::assertNull($idempotency->get('k', 2_000));
        self::assertSame(2_000, $redis->expiries['nimbus:idempotency:k']);

        $cache = new RedisResponseCacheStore($redis);
        $cache->put('page', new CacheRecord($snapshot, 2_000));
        self::assertEquals($snapshot, $cache->get('page', 1_000)?->response);
        $cache->forget('page');
        self::assertNull($cache->get('page', 1_000));
    }

    public function testThePoolReusesIdleConnections(): void
    {
        $opened = 0;
        $pool = new RedisPool(static function () use (&$opened): Connection {
            ++$opened;
            return new MemoryConnection();
        }, maxIdle: 1);

        $pool->command(['SET', 'a', '1']);
        self::assertSame('1', $pool->command(['GET', 'a']));
        self::assertSame(1, $opened);
    }

    public function testTokenBucketsAreSharedThroughRedis(): void
    {
        $host = getenv('NIMBUS_TEST_REDIS');
        if (!is_string($host) || $host === '') {
            self::markTestSkipped('Set NIMBUS_TEST_REDIS=host:port to run against Redis.');
        }
        $redis = new SocketConnection($host);
        $redis->command(['DEL', 'nimbus:rate:test']);
        $first = new RedisRateLimitStore($redis);
        $second = new RedisRateLimitStore(new SocketConnection($host));

        self::assertTrue($first->consume('test', 1, 2, 100.0)->allowed);
        self::assertTrue($second->consume('test', 1, 2, 100.0)->allowed);
        $denied = $first->consume('test', 1, 2, 100.0);
        self::assertFalse($denied->allowed);
        self::assertSame(1, $denied->retryAfterSeconds);
        self::assertTrue($second->consume('test', 1, 2, 101.0)->allowed);
    }
}

final class MemoryConnection implements Connection
{
    /** @var array<string, string> */
    private array $values = [];

    /** @var array<string, int> */
    public array $expiries = [];

    public function command(array $arguments): mixed
    {
        $key = (string) $arguments[1];
        return match ($arguments[0]) {
            'GET' => $this->values[$key] ?? null,
            'SET' => $this->set($key, (string) $arguments[2], isset($arguments[4]) ? (int) $arguments[4] : null),
            'DEL' => $this->delete($key),
            default => throw new \LogicException("Unsupported command {$arguments[0]}."),
        };
    }

    private function delete(string $key): int
    {
        $existed = isset($this->values[$key]);
        unset($this->values[$key]);
        return (int) $existed;
    }

    private function set(string $key, string $value, ?int $expiresAt): string
    {
        $this->values[$key] = $value;
        if ($expiresAt !== null) {
            $this->expiries[$key] = $expiresAt;
        }
        return 'OK';
    }
}

/** A minimal RESP client for the opt-in Redis test. */
final class SocketConnection implements Connection
{
    /** @var resource */
    private $stream;

    public function __construct(string $address)
    {
        $stream = stream_socket_client('tcp://' . $address, $code, $message, 2.0);
        if ($stream === false) {
            throw new \RuntimeException("Cannot reach Redis at {$address}: {$message}");
        }
        $this->stream = $stream;
    }

    public function command(array $arguments): mixed
    {
        $payload = '*' . count($arguments) . "\r\n";
        foreach ($arguments as $argument) {
            $argument = (string) $argument;
            $payload .= '$' . strlen($argument) . "\r\n" . $argument . "\r\n";
        }
        fwrite($this->stream, $payload);
        return $this->read();
    }

    private function read(): mixed
    {
        $line = fgets($this->stream);
        if ($line === false) {
            throw new \RuntimeException('Redis closed the connection.');
        }
        $line = substr($line, 0, -2);
        $rest = substr($line, 1);
        return match ($line[0]) {
            '+' => $rest,
            '-' => throw new \RuntimeException($rest),
            ':' => (int) $rest,
            '$' => $rest === '-1' ? null : substr((string) fread($this->stream, max(1, (int) $rest + 2)), 0, -2),
            '*' => (int) $rest < 1 ? [] : array_map(fn (): mixed => $this->read(), range(1, (int) $rest)),
            default => throw new \RuntimeException('Unexpected Redis reply.'),
        };
    }
}
