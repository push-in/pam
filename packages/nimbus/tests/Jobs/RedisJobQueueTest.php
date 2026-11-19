<?php

declare(strict_types=1);

namespace Nimbus\Tests\Jobs;

use Nimbus\Jobs\FailedJobRepository;
use Nimbus\Jobs\JobCodec;
use Nimbus\Jobs\JobQueue;
use Nimbus\Jobs\RedisJobQueue;
use Nimbus\Redis\Connection;
use Nimbus\Redis\LuaScript;
use Nimbus\Tests\Redis\SocketConnection;
use PHPUnit\Framework\Attributes\CoversClass;
use PHPUnit\Framework\TestCase;

/** Runs against Redis when NIMBUS_TEST_REDIS=host:port is set. */
#[CoversClass(RedisJobQueue::class)]
#[CoversClass(LuaScript::class)]
final class RedisJobQueueTest extends TestCase
{
    use PersistentQueueContract;

    private ?SocketConnection $redis = null;

    private string $prefix = '';

    protected function setUp(): void
    {
        $this->prefix = 'nimbus:test:' . bin2hex(random_bytes(6)) . ':';
        $host = getenv('NIMBUS_TEST_REDIS');
        if (is_string($host) && $host !== '') {
            $this->redis = new SocketConnection($host);
        } elseif (str_starts_with($this->name(), 'test') && !str_starts_with($this->name(), 'testScripts')) {
            self::markTestSkipped('Set NIMBUS_TEST_REDIS=host:port to run against Redis.');
        }
    }

    protected function tearDown(): void
    {
        if ($this->redis === null) {
            return;
        }
        $keys = $this->redis->command(['KEYS', $this->prefix . '*']);
        if (is_array($keys) && $keys !== []) {
            $this->redis->command(['DEL', ...array_values(array_filter($keys, is_string(...)))]);
        }
    }

    public function testScriptsAreSentByDigestAndLoadedOnNoscript(): void
    {
        $redis = new class implements Connection {
            /** @var list<list<string|int|float>> */
            public array $commands = [];

            public function command(array $arguments): mixed
            {
                $this->commands[] = $arguments;
                if ($arguments[0] === 'EVALSHA') {
                    throw new \RuntimeException('NOSCRIPT No matching script.');
                }
                return 1;
            }
        };
        $queue = new RedisJobQueue($redis, self::codec(), 'mail', clock: static fn (): int => 100);

        $envelope = $queue->dispatch(new ReportJob(1), delaySeconds: 5);

        self::assertSame(105, $envelope->availableAt);
        self::assertCount(2, $redis->commands);
        [$digest, $eval] = $redis->commands;
        self::assertSame('EVALSHA', $digest[0]);
        self::assertSame('EVAL', $eval[0]);
        self::assertSame(4, $eval[2]);
        $keys = array_slice($eval, 3, 4);
        self::assertSame(
            ['nimbus:queue:{mail}:pending', 'nimbus:queue:{mail}:payloads', 'nimbus:queue:{mail}:attempts', 'nimbus:queue:{mail}:maximum'],
            $keys,
            'Every key shares the hash tag of its queue.',
        );
        self::assertSame([$envelope->id, '{"schema":1,"name":"reports.send","payload":{"reportId":1}}', 3, 105], array_slice($eval, 7));
    }

    private function queue(string $name = 'default', ?JobCodec $codec = null): JobQueue&FailedJobRepository
    {
        $redis = $this->redis ?? throw new \LogicException('Redis is not configured.');
        return new RedisJobQueue($redis, $codec ?? self::codec(), $name, $this->prefix, $this->clock());
    }
}
