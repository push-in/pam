<?php

declare(strict_types=1);

namespace Nimbus\Jobs;

use Arcano\Database\DatabaseInterface;
use Nimbus\Database\ArcanoDatabaseProvider;

/**
 * A queue in the `jobs` and `failed_jobs` tables of an Arcano database
 * (see QueueTablesMigration).
 *
 * On Postgres and MySQL a reservation locks its row with
 * `FOR UPDATE SKIP LOCKED`, so concurrent workers never wait on each other's
 * rows. Every driver also guards each transition with a compare-and-swap on the
 * attempt count, which is what keeps SQLite (no row locks) and a worker whose
 * lease expired from taking or touching a job that another worker now owns.
 */
final class DatabaseJobQueue implements JobQueue, FailedJobRepository
{
    private const int MAXIMUM_DELAY_SECONDS = 604_800;
    private const int MAXIMUM_LEASE_SECONDS = 3_600;
    private const string JOBS = 'jobs';
    private const string FAILED = 'failed_jobs';

    /** @var \Closure(): int */
    private readonly \Closure $clock;

    /** @var \WeakMap<JobEnvelope, string> Encoded payload of each reservation. */
    private \WeakMap $reserved;

    /** @param (\Closure(): int)|null $clock Seconds since the epoch; defaults to time(). */
    public function __construct(
        private readonly ArcanoDatabaseProvider|DatabaseInterface $database,
        private readonly JobCodec $codec,
        private readonly string $queue = 'default',
        private readonly ?string $connection = null,
        ?\Closure $clock = null,
    ) {
        if (preg_match('/^[A-Za-z0-9][A-Za-z0-9._-]{0,63}$/D', $queue) !== 1) {
            throw new \InvalidArgumentException('Queue names must be bounded identifiers.');
        }
        $this->clock = $clock ?? time(...);
        $this->reserved = new \WeakMap();
    }

    public function dispatch(object $job, int $maximumAttempts = 3, int $delaySeconds = 0): JobEnvelope
    {
        if (!$job instanceof SerializableJob) {
            throw new \InvalidArgumentException('Persistent queues accept only ' . SerializableJob::class . ' jobs.');
        }
        if ($delaySeconds < 0 || $delaySeconds > self::MAXIMUM_DELAY_SECONDS) {
            throw new \InvalidArgumentException('Job delay must be between zero and seven days.');
        }
        $now = ($this->clock)();
        $envelope = new JobEnvelope(bin2hex(random_bytes(16)), $job, $maximumAttempts, $now + $delaySeconds);
        $this->insertJob($this->db(), $envelope->id, $this->codec->encode($job), $maximumAttempts, $envelope->availableAt, $now);
        return $envelope;
    }

    public function reserve(int $now, int $leaseSeconds): ?JobEnvelope
    {
        if ($now < 0 || $leaseSeconds < 1 || $leaseSeconds > self::MAXIMUM_LEASE_SECONDS) {
            throw new \InvalidArgumentException('Reservation time must be non-negative and lease must be positive.');
        }
        $db = $this->db();
        $locking = in_array($db->getDriver()->getType(), ['Postgres', 'MySQL'], true);
        for ($round = 0; $round < 10; ++$round) {
            $claim = $locking
                ? $db->transaction(fn (): array|false|null => $this->claim($db, $now, $leaseSeconds, true))
                : $this->claim($db, $now, $leaseSeconds, false);
            if ($claim === null) {
                return null;
            }
            if ($claim === false) {
                continue;
            }
            /** @var array{id: string, payload: string, attempts: int, maximum: int} $claim */
            try {
                $job = $this->codec->decode($claim['payload']);
            } catch (\Throwable $failure) {
                // A payload that no longer decodes cannot succeed on a later attempt.
                $this->moveToFailed($db, $claim['id'], $claim['attempts'], $claim['payload'], $claim['maximum'], $failure::class, $now);
                continue;
            }
            $envelope = new JobEnvelope($claim['id'], $job, max(1, $claim['maximum']), $now);
            $envelope->state = JobState::Processing;
            $envelope->attempts = $claim['attempts'];
            $envelope->leaseUntil = $now + $leaseSeconds;
            $this->reserved[$envelope] = $claim['payload'];
            return $envelope;
        }
        return null;
    }

    public function complete(JobEnvelope $envelope): void
    {
        $this->assertReserved($envelope);
        $this->db()->delete(self::JOBS, ['id' => $envelope->id, 'attempts' => $envelope->attempts])->run();
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
        $this->db()->update(self::JOBS, [
            'available_at' => $availableAt,
            'reserved_until' => null,
            'last_failure' => $failureClass,
        ], ['id' => $envelope->id, 'attempts' => $envelope->attempts])->run();
        $envelope->state = JobState::Pending;
        $envelope->availableAt = $availableAt;
        $envelope->leaseUntil = null;
        $envelope->lastFailureClass = $failureClass;
        unset($this->reserved[$envelope]);
    }

    public function deadLetter(JobEnvelope $envelope, ?\Throwable $failure = null): void
    {
        $payload = $this->assertReserved($envelope);
        $failureClass = $failure === null ? null : $failure::class;
        $this->moveToFailed(
            $this->db(),
            $envelope->id,
            $envelope->attempts,
            $payload,
            $envelope->maximumAttempts,
            $failureClass,
            ($this->clock)(),
        );
        $envelope->state = JobState::DeadLetter;
        $envelope->leaseUntil = null;
        $envelope->lastFailureClass = $failureClass;
        unset($this->reserved[$envelope]);
    }

    /** Jobs waiting or reserved on this queue. */
    public function size(): int
    {
        return $this->db()->select()->from(self::JOBS)->where('queue', $this->queue)->count();
    }

    public function failed(int $limit = 100): array
    {
        if ($limit < 1 || $limit > 1_000) {
            throw new \InvalidArgumentException('Failed jobs are listed 1 to 1,000 at a time.');
        }
        $rows = $this->db()
            ->select(['id', 'queue', 'payload', 'attempts', 'failure_class', 'failed_at'])
            ->from(self::FAILED)
            ->where('queue', $this->queue)
            ->orderBy('failed_at', 'DESC')
            ->orderBy('id', 'DESC')
            ->limit($limit)
            ->fetchAll();
        $failed = [];
        foreach ($rows as $row) {
            if (!is_array($row)) {
                continue;
            }
            $failure = $row['failure_class'] ?? null;
            $failed[] = new FailedJob(
                self::string($row['id'] ?? ''),
                self::string($row['queue'] ?? ''),
                self::string($row['payload'] ?? ''),
                self::int($row['attempts'] ?? 0),
                is_string($failure) && $failure !== '' ? $failure : null,
                self::int($row['failed_at'] ?? 0),
            );
        }
        return $failed;
    }

    public function retryFailed(string $id): bool
    {
        $db = $this->db();
        return (bool) $db->transaction(function () use ($db, $id): bool {
            $rows = $db->select(['payload', 'max_attempts'])
                ->from(self::FAILED)
                ->where('id', $id)
                ->where('queue', $this->queue)
                ->limit(1)
                ->fetchAll();
            $row = $rows[0] ?? null;
            if (!is_array($row) || $db->delete(self::FAILED, ['id' => $id, 'queue' => $this->queue])->run() !== 1) {
                return false;
            }
            $now = ($this->clock)();
            $this->insertJob($db, $id, self::string($row['payload'] ?? ''), max(1, self::int($row['max_attempts'] ?? 1)), $now, $now);
            return true;
        });
    }

    public function forgetFailed(string $id): bool
    {
        return $this->db()->delete(self::FAILED, ['id' => $id, 'queue' => $this->queue])->run() === 1;
    }

    /**
     * Takes the next available row. Null when the queue has nothing due, false when
     * another worker won the row (or it was moved to the failed jobs) and the
     * caller should look again.
     *
     * @return array{id: string, payload: string, attempts: int, maximum: int}|false|null
     */
    private function claim(DatabaseInterface $db, int $now, int $leaseSeconds, bool $locking): array|false|null
    {
        $row = $locking ? $this->lockNext($db, $now) : $this->peekNext($db, $now);
        if ($row === null) {
            return null;
        }
        $id = self::string($row['id'] ?? '');
        $payload = self::string($row['payload'] ?? '');
        $attempts = self::int($row['attempts'] ?? 0);
        $maximum = self::int($row['max_attempts'] ?? 1);
        $reservedUntil = $row['reserved_until'] ?? null;
        if ($attempts >= $maximum) {
            // A worker died holding the last attempt: the job is failed, not retried.
            $failure = $row['last_failure'] ?? null;
            $this->moveToFailed($db, $id, $attempts, $payload, $maximum, is_string($failure) ? $failure : null, $now);
            return false;
        }
        $claimed = $db->update(self::JOBS, [
            'attempts' => $attempts + 1,
            'reserved_until' => $now + $leaseSeconds,
        ], [
            'id' => $id,
            'attempts' => $attempts,
            'reserved_until' => $reservedUntil === null ? null : self::int($reservedUntil),
        ])->run();
        if ($claimed !== 1) {
            return false;
        }
        return ['id' => $id, 'payload' => $payload, 'attempts' => $attempts + 1, 'maximum' => $maximum];
    }

    /** @return array<array-key, mixed>|null */
    private function peekNext(DatabaseInterface $db, int $now): ?array
    {
        $rows = $db->select(['id', 'payload', 'attempts', 'max_attempts', 'reserved_until', 'last_failure'])
            ->from(self::JOBS)
            ->where('queue', $this->queue)
            ->where(static function ($query) use ($now): void {
                $query->where(static function ($available) use ($now): void {
                    $available->where('reserved_until', null)->where('available_at', '<=', $now);
                })->orWhere('reserved_until', '<=', $now);
            })
            ->orderBy('available_at')
            ->orderBy('id')
            ->limit(1)
            ->fetchAll();
        $row = $rows[0] ?? null;
        return is_array($row) ? $row : null;
    }

    /** @return array<array-key, mixed>|null */
    private function lockNext(DatabaseInterface $db, int $now): ?array
    {
        $table = $db->getPrefix() . self::JOBS;
        $row = $db->query(
            "SELECT id, payload, attempts, max_attempts, reserved_until, last_failure FROM {$table}"
            . ' WHERE queue = ? AND ((reserved_until IS NULL AND available_at <= ?) OR reserved_until <= ?)'
            . ' ORDER BY available_at, id LIMIT 1 FOR UPDATE SKIP LOCKED',
            [$this->queue, $now, $now],
        )->fetch();
        return is_array($row) ? $row : null;
    }

    private function insertJob(DatabaseInterface $db, string $id, string $payload, int $maximumAttempts, int $availableAt, int $now): void
    {
        $db->insert(self::JOBS)->values([
            'id' => $id,
            'queue' => $this->queue,
            'payload' => $payload,
            'attempts' => 0,
            'max_attempts' => $maximumAttempts,
            'available_at' => $availableAt,
            'reserved_until' => null,
            'last_failure' => null,
            'created_at' => $now,
        ])->run();
    }

    private function moveToFailed(
        DatabaseInterface $db,
        string $id,
        int $attempts,
        string $payload,
        int $maximumAttempts,
        ?string $failureClass,
        int $now,
    ): void {
        $db->transaction(function () use ($db, $id, $attempts, $payload, $maximumAttempts, $failureClass, $now): void {
            if ($db->delete(self::JOBS, ['id' => $id, 'attempts' => $attempts])->run() !== 1) {
                return;
            }
            $db->insert(self::FAILED)->values([
                'id' => $id,
                'queue' => $this->queue,
                'payload' => $payload,
                'attempts' => $attempts,
                'max_attempts' => $maximumAttempts,
                'failure_class' => $failureClass === null ? null : substr($failureClass, 0, 255),
                'failed_at' => $now,
            ])->run();
        });
    }

    private function assertReserved(JobEnvelope $envelope): string
    {
        $payload = $this->reserved[$envelope] ?? null;
        if ($payload === null || $envelope->state !== JobState::Processing) {
            throw new \LogicException('Only an envelope reserved by this queue may be transitioned.');
        }
        return $payload;
    }

    private function db(): DatabaseInterface
    {
        return $this->database instanceof ArcanoDatabaseProvider
            ? $this->database->database($this->connection)
            : $this->database;
    }

    private static function int(mixed $value): int
    {
        return is_numeric($value) ? (int) $value : 0;
    }

    private static function string(mixed $value): string
    {
        return is_scalar($value) ? (string) $value : '';
    }
}
