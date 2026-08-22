<?php

declare(strict_types=1);

namespace Pam\Api\Jobs;

final class MemoryJobDispatcher implements JobQueue
{
    private const int MAXIMUM_DELAY_SECONDS = 604_800;

    /** @var list<JobEnvelope> */
    private array $queue = [];

    public function __construct(private readonly int $maximumQueuedJobs = 10_000)
    {
        if ($maximumQueuedJobs < 1) {
            throw new \InvalidArgumentException('Maximum queued jobs must be positive.');
        }
    }

    public function dispatch(object $job, int $maximumAttempts = 3, int $delaySeconds = 0): JobEnvelope
    {
        if (
            $delaySeconds < 0
            || $delaySeconds > self::MAXIMUM_DELAY_SECONDS
            || count($this->queue) >= $this->maximumQueuedJobs
        ) {
            throw new \RuntimeException('Job cannot be added to the bounded queue.');
        }
        $envelope = new JobEnvelope(
            bin2hex(random_bytes(16)),
            $job,
            $maximumAttempts,
            time() + $delaySeconds,
        );
        $this->queue[] = $envelope;
        return $envelope;
    }

    public function reserve(int $now, int $leaseSeconds): ?JobEnvelope
    {
        if ($now < 0 || $leaseSeconds < 1) {
            throw new \InvalidArgumentException('Reservation time must be non-negative and lease must be positive.');
        }
        foreach ($this->queue as $envelope) {
            $leaseExpired = $envelope->state === JobState::Processing
                && $envelope->leaseUntil !== null
                && $envelope->leaseUntil <= $now;
            if (($envelope->state !== JobState::Pending && !$leaseExpired) || $envelope->availableAt > $now) {
                continue;
            }
            if ($envelope->attempts >= $envelope->maximumAttempts) {
                $envelope->state = JobState::DeadLetter;
                $envelope->leaseUntil = null;
                continue;
            }
            $envelope->state = JobState::Processing;
            ++$envelope->attempts;
            $envelope->leaseUntil = $now + $leaseSeconds;
            return $envelope;
        }
        return null;
    }

    public function complete(JobEnvelope $envelope): void
    {
        $this->assertReserved($envelope);
        $envelope->state = JobState::Completed;
        $envelope->leaseUntil = null;
        $this->remove($envelope);
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
        $envelope->state = JobState::Pending;
        $envelope->availableAt = $availableAt;
        $envelope->leaseUntil = null;
        $envelope->lastFailureClass = $failure === null ? null : $failure::class;
    }

    public function deadLetter(JobEnvelope $envelope, ?\Throwable $failure = null): void
    {
        $this->assertReserved($envelope);
        $envelope->state = JobState::DeadLetter;
        $envelope->leaseUntil = null;
        $envelope->lastFailureClass = $failure === null ? null : $failure::class;
    }

    /** @return list<JobEnvelope> */
    public function pending(): array
    {
        return array_values(array_filter(
            $this->queue,
            static fn (JobEnvelope $envelope): bool => $envelope->state === JobState::Pending,
        ));
    }

    /** @return list<JobEnvelope> */
    public function deadLetters(): array
    {
        return array_values(array_filter(
            $this->queue,
            static fn (JobEnvelope $envelope): bool => $envelope->state === JobState::DeadLetter,
        ));
    }

    private function assertReserved(JobEnvelope $envelope): void
    {
        foreach ($this->queue as $queued) {
            if ($queued === $envelope && $queued->state === JobState::Processing) {
                return;
            }
        }
        throw new \LogicException('Only an envelope reserved by this queue may be transitioned.');
    }

    private function remove(JobEnvelope $envelope): void
    {
        foreach ($this->queue as $index => $queued) {
            if ($queued === $envelope) {
                array_splice($this->queue, $index, 1);
                return;
            }
        }
    }
}
