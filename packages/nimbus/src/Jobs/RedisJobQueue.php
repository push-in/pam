<?php

declare(strict_types=1);

namespace Nimbus\Jobs;

use Nimbus\Redis\Connection;
use Nimbus\Redis\LuaScript;

/**
 * A queue shared by every worker and server through Redis.
 *
 * Pending jobs live in a sorted set scored by the time they become available,
 * so delayed jobs and retries need no extra structure. A reservation moves the
 * job to a second sorted set scored by its lease; a worker that dies mid-job
 * leaves the lease to expire and the next reservation puts the job back. Every
 * transition runs in one Lua script, and completions, releases and dead letters
 * only apply while the attempt count still matches the reservation, so a worker
 * whose lease expired cannot touch the job of the worker that took it over.
 * All keys share one hash tag and live in the same Redis Cluster slot.
 */
final class RedisJobQueue implements JobQueue, FailedJobRepository
{
    private const int MAXIMUM_DELAY_SECONDS = 604_800;
    private const int MAXIMUM_LEASE_SECONDS = 3_600;

    private const string DISPATCH = <<<'LUA'
        redis.call('HSET', KEYS[2], ARGV[1], ARGV[2])
        redis.call('HSET', KEYS[3], ARGV[1], 0)
        redis.call('HSET', KEYS[4], ARGV[1], ARGV[3])
        redis.call('ZADD', KEYS[1], ARGV[4], ARGV[1])
        return 1
        LUA;

    private const string RESERVE = <<<'LUA'
        local now = tonumber(ARGV[1])
        for _, id in ipairs(redis.call('ZRANGEBYSCORE', KEYS[2], '-inf', now, 'LIMIT', 0, 100)) do
            redis.call('ZREM', KEYS[2], id)
            redis.call('ZADD', KEYS[1], now, id)
        end
        for _ = 1, 100 do
            local ids = redis.call('ZRANGEBYSCORE', KEYS[1], '-inf', now, 'LIMIT', 0, 1)
            if #ids == 0 then
                return false
            end
            local id = ids[1]
            redis.call('ZREM', KEYS[1], id)
            local payload = redis.call('HGET', KEYS[3], id)
            if payload then
                local attempts = tonumber(redis.call('HGET', KEYS[4], id) or '0')
                local maximum = tonumber(redis.call('HGET', KEYS[5], id) or '1')
                if attempts < maximum then
                    attempts = attempts + 1
                    redis.call('HSET', KEYS[4], id, attempts)
                    redis.call('ZADD', KEYS[2], now + tonumber(ARGV[2]), id)
                    return {id, payload, attempts, maximum}
                end
                local failure = redis.call('HGET', KEYS[6], id) or ''
                redis.call('HSET', KEYS[7], id, cjson.encode({
                    queue = ARGV[3], payload = payload, attempts = attempts,
                    maximum = maximum, failure = failure, failedAt = now,
                }))
                redis.call('ZADD', KEYS[8], now, id)
                redis.call('HDEL', KEYS[3], id)
                redis.call('HDEL', KEYS[4], id)
                redis.call('HDEL', KEYS[5], id)
                redis.call('HDEL', KEYS[6], id)
            end
        end
        return false
        LUA;

    private const string COMPLETE = <<<'LUA'
        if tonumber(redis.call('HGET', KEYS[3], ARGV[1]) or '-1') ~= tonumber(ARGV[2]) then
            return 0
        end
        redis.call('ZREM', KEYS[1], ARGV[1])
        redis.call('ZREM', KEYS[2], ARGV[1])
        redis.call('HDEL', KEYS[3], ARGV[1])
        redis.call('HDEL', KEYS[4], ARGV[1])
        redis.call('HDEL', KEYS[5], ARGV[1])
        redis.call('HDEL', KEYS[6], ARGV[1])
        return 1
        LUA;

    private const string RELEASE = <<<'LUA'
        if tonumber(redis.call('HGET', KEYS[3], ARGV[1]) or '-1') ~= tonumber(ARGV[2]) then
            return 0
        end
        redis.call('ZREM', KEYS[2], ARGV[1])
        redis.call('ZADD', KEYS[1], ARGV[3], ARGV[1])
        if ARGV[4] == '' then
            redis.call('HDEL', KEYS[4], ARGV[1])
        else
            redis.call('HSET', KEYS[4], ARGV[1], ARGV[4])
        end
        return 1
        LUA;

    private const string DEAD_LETTER = <<<'LUA'
        if tonumber(redis.call('HGET', KEYS[4], ARGV[1]) or '-1') ~= tonumber(ARGV[2]) then
            return 0
        end
        local maximum = tonumber(redis.call('HGET', KEYS[5], ARGV[1]) or '1')
        redis.call('HSET', KEYS[7], ARGV[1], cjson.encode({
            queue = ARGV[5], payload = redis.call('HGET', KEYS[3], ARGV[1]) or '',
            attempts = tonumber(ARGV[2]), maximum = maximum, failure = ARGV[3],
            failedAt = tonumber(ARGV[4]),
        }))
        redis.call('ZADD', KEYS[8], ARGV[4], ARGV[1])
        redis.call('ZREM', KEYS[1], ARGV[1])
        redis.call('ZREM', KEYS[2], ARGV[1])
        redis.call('HDEL', KEYS[3], ARGV[1])
        redis.call('HDEL', KEYS[4], ARGV[1])
        redis.call('HDEL', KEYS[5], ARGV[1])
        redis.call('HDEL', KEYS[6], ARGV[1])
        return 1
        LUA;

    private const string RETRY_FAILED = <<<'LUA'
        local record = redis.call('HGET', KEYS[5], ARGV[1])
        if not record then
            return 0
        end
        local failed = cjson.decode(record)
        redis.call('HSET', KEYS[2], ARGV[1], failed.payload)
        redis.call('HSET', KEYS[3], ARGV[1], 0)
        redis.call('HSET', KEYS[4], ARGV[1], failed.maximum)
        redis.call('ZADD', KEYS[1], ARGV[2], ARGV[1])
        redis.call('HDEL', KEYS[5], ARGV[1])
        redis.call('ZREM', KEYS[6], ARGV[1])
        return 1
        LUA;

    private readonly LuaScript $dispatchScript;
    private readonly LuaScript $reserveScript;
    private readonly LuaScript $completeScript;
    private readonly LuaScript $releaseScript;
    private readonly LuaScript $deadLetterScript;
    private readonly LuaScript $retryFailedScript;

    /** @var \Closure(): int */
    private readonly \Closure $clock;

    /** @var \WeakMap<JobEnvelope, true> */
    private \WeakMap $reserved;

    /** @param (\Closure(): int)|null $clock Seconds since the epoch; defaults to time(). */
    public function __construct(
        private readonly Connection $redis,
        private readonly JobCodec $codec,
        private readonly string $queue = 'default',
        private readonly string $prefix = 'nimbus:queue:',
        ?\Closure $clock = null,
    ) {
        if (preg_match('/^[A-Za-z0-9][A-Za-z0-9._-]{0,63}$/D', $queue) !== 1) {
            throw new \InvalidArgumentException('Queue names must be bounded identifiers.');
        }
        $this->clock = $clock ?? time(...);
        $this->reserved = new \WeakMap();
        $this->dispatchScript = new LuaScript(self::DISPATCH);
        $this->reserveScript = new LuaScript(self::RESERVE);
        $this->completeScript = new LuaScript(self::COMPLETE);
        $this->releaseScript = new LuaScript(self::RELEASE);
        $this->deadLetterScript = new LuaScript(self::DEAD_LETTER);
        $this->retryFailedScript = new LuaScript(self::RETRY_FAILED);
    }

    public function dispatch(object $job, int $maximumAttempts = 3, int $delaySeconds = 0): JobEnvelope
    {
        if (!$job instanceof SerializableJob) {
            throw new \InvalidArgumentException('Persistent queues accept only ' . SerializableJob::class . ' jobs.');
        }
        if ($delaySeconds < 0 || $delaySeconds > self::MAXIMUM_DELAY_SECONDS) {
            throw new \InvalidArgumentException('Job delay must be between zero and seven days.');
        }
        $envelope = new JobEnvelope(bin2hex(random_bytes(16)), $job, $maximumAttempts, ($this->clock)() + $delaySeconds);
        $this->dispatchScript->run(
            $this->redis,
            [$this->key('pending'), $this->key('payloads'), $this->key('attempts'), $this->key('maximum')],
            [$envelope->id, $this->codec->encode($job), $maximumAttempts, $envelope->availableAt],
        );
        return $envelope;
    }

    public function reserve(int $now, int $leaseSeconds): ?JobEnvelope
    {
        if ($now < 0 || $leaseSeconds < 1 || $leaseSeconds > self::MAXIMUM_LEASE_SECONDS) {
            throw new \InvalidArgumentException('Reservation time must be non-negative and lease must be positive.');
        }
        for ($skipped = 0; $skipped < 10; ++$skipped) {
            $reply = $this->reserveScript->run($this->redis, [
                $this->key('pending'),
                $this->key('reserved'),
                $this->key('payloads'),
                $this->key('attempts'),
                $this->key('maximum'),
                $this->key('failures'),
                $this->key('failed'),
                $this->key('failed-index'),
            ], [$now, $leaseSeconds, $this->queue]);
            if ($reply === null || $reply === false) {
                return null;
            }
            if (
                !is_array($reply)
                || count($reply) !== 4
                || !is_string($reply[0] ?? null)
                || !is_string($reply[1] ?? null)
                || !is_numeric($reply[2] ?? null)
                || !is_numeric($reply[3] ?? null)
            ) {
                throw new \UnexpectedValueException('Unexpected Redis queue reply.');
            }
            [$id, $payload, $attempts, $maximum] = [$reply[0], $reply[1], (int) $reply[2], (int) $reply[3]];
            try {
                $job = $this->codec->decode($payload);
            } catch (\Throwable $failure) {
                // A payload that no longer decodes cannot succeed on a later attempt.
                $this->deadLetterById($id, $attempts, $failure::class, $now);
                continue;
            }
            $envelope = new JobEnvelope($id, $job, max(1, $maximum), $now);
            $envelope->state = JobState::Processing;
            $envelope->attempts = $attempts;
            $envelope->leaseUntil = $now + $leaseSeconds;
            $this->reserved[$envelope] = true;
            return $envelope;
        }
        return null;
    }

    public function complete(JobEnvelope $envelope): void
    {
        $this->assertReserved($envelope);
        $this->completeScript->run($this->redis, [
            $this->key('pending'),
            $this->key('reserved'),
            $this->key('attempts'),
            $this->key('payloads'),
            $this->key('maximum'),
            $this->key('failures'),
        ], [$envelope->id, $envelope->attempts]);
        $envelope->state = JobState::Completed;
        $envelope->leaseUntil = null;
        unset($this->reserved[$envelope]);
    }

    public function release(JobEnvelope $envelope, int $availableAt, ?\Throwable $failure = null): void
    {
        $this->assertReserved($envelope);
        if ($availableAt < 0) {
            throw new \InvalidArgumentException('Availability timestamp cannot be negative.');
        }
        if ($envelope->attempts >= $envelope->maximumAttempts) {
            $this->deadLetter($envelope, $failure);
            return;
        }
        $failureClass = $failure === null ? null : $failure::class;
        $this->releaseScript->run($this->redis, [
            $this->key('pending'),
            $this->key('reserved'),
            $this->key('attempts'),
            $this->key('failures'),
        ], [$envelope->id, $envelope->attempts, $availableAt, $failureClass ?? '']);
        $envelope->state = JobState::Pending;
        $envelope->availableAt = $availableAt;
        $envelope->leaseUntil = null;
        $envelope->lastFailureClass = $failureClass;
        unset($this->reserved[$envelope]);
    }

    public function deadLetter(JobEnvelope $envelope, ?\Throwable $failure = null): void
    {
        $this->assertReserved($envelope);
        $failureClass = $failure === null ? null : $failure::class;
        $this->deadLetterById($envelope->id, $envelope->attempts, $failureClass ?? '', ($this->clock)());
        $envelope->state = JobState::DeadLetter;
        $envelope->leaseUntil = null;
        $envelope->lastFailureClass = $failureClass;
        unset($this->reserved[$envelope]);
    }

    /** Jobs waiting or reserved on this queue. */
    public function size(): int
    {
        $pending = $this->redis->command(['ZCARD', $this->key('pending')]);
        $reserved = $this->redis->command(['ZCARD', $this->key('reserved')]);
        return (is_int($pending) ? $pending : 0) + (is_int($reserved) ? $reserved : 0);
    }

    public function failed(int $limit = 100): array
    {
        if ($limit < 1 || $limit > 1_000) {
            throw new \InvalidArgumentException('Failed jobs are listed 1 to 1,000 at a time.');
        }
        $ids = $this->redis->command(['ZREVRANGE', $this->key('failed-index'), 0, $limit - 1]);
        if (!is_array($ids) || $ids === []) {
            return [];
        }
        $ids = array_values(array_filter($ids, is_string(...)));
        if ($ids === []) {
            return [];
        }
        $records = $this->redis->command(['HMGET', $this->key('failed'), ...$ids]);
        if (!is_array($records)) {
            throw new \UnexpectedValueException('Unexpected Redis failed-jobs reply.');
        }
        $failed = [];
        foreach ($ids as $index => $id) {
            $record = $records[$index] ?? null;
            if (!is_string($record)) {
                continue;
            }
            $data = json_decode($record, true);
            if (!is_array($data) || !is_string($data['payload'] ?? null)) {
                continue;
            }
            $failure = $data['failure'] ?? '';
            $failed[] = new FailedJob(
                $id,
                is_string($data['queue'] ?? null) ? $data['queue'] : $this->queue,
                $data['payload'],
                is_numeric($data['attempts'] ?? null) ? (int) $data['attempts'] : 0,
                is_string($failure) && $failure !== '' ? $failure : null,
                is_numeric($data['failedAt'] ?? null) ? (int) $data['failedAt'] : 0,
            );
        }
        return $failed;
    }

    public function retryFailed(string $id): bool
    {
        return $this->retryFailedScript->run($this->redis, [
            $this->key('pending'),
            $this->key('payloads'),
            $this->key('attempts'),
            $this->key('maximum'),
            $this->key('failed'),
            $this->key('failed-index'),
        ], [$id, ($this->clock)()]) === 1;
    }

    public function forgetFailed(string $id): bool
    {
        $removed = $this->redis->command(['HDEL', $this->key('failed'), $id]);
        $this->redis->command(['ZREM', $this->key('failed-index'), $id]);
        return $removed === 1;
    }

    private function deadLetterById(string $id, int $attempts, string $failureClass, int $now): void
    {
        $this->deadLetterScript->run($this->redis, [
            $this->key('pending'),
            $this->key('reserved'),
            $this->key('payloads'),
            $this->key('attempts'),
            $this->key('maximum'),
            $this->key('failures'),
            $this->key('failed'),
            $this->key('failed-index'),
        ], [$id, $attempts, $failureClass, $now, $this->queue]);
    }

    private function assertReserved(JobEnvelope $envelope): void
    {
        if (!isset($this->reserved[$envelope]) || $envelope->state !== JobState::Processing) {
            throw new \LogicException('Only an envelope reserved by this queue may be transitioned.');
        }
    }

    private function key(string $structure): string
    {
        return $this->prefix . '{' . $this->queue . '}:' . $structure;
    }
}
