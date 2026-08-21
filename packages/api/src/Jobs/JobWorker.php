<?php

declare(strict_types=1);

namespace Pam\Api\Jobs;

final readonly class JobWorker
{
    private const int MAXIMUM_LEASE_SECONDS = 3_600;
    private const int MAXIMUM_RETRY_DELAY_SECONDS = 604_800;

    /** @var \Closure(object): JobOutcome */
    private \Closure $handler;

    /**
     * @param JobHandler|callable(object): JobOutcome $handler
     */
    public function __construct(
        private JobQueue $queue,
        JobHandler|callable $handler,
        private int $leaseSeconds = 30,
        private int $retryDelaySeconds = 5,
    ) {
        if (
            $leaseSeconds < 1
            || $leaseSeconds > self::MAXIMUM_LEASE_SECONDS
            || $retryDelaySeconds < 0
            || $retryDelaySeconds > self::MAXIMUM_RETRY_DELAY_SECONDS
        ) {
            throw new \InvalidArgumentException('Worker lease and retry delay are invalid.');
        }
        $this->handler = $handler instanceof JobHandler
            ? $handler->handle(...)
            : \Closure::fromCallable($handler);
    }

    public function runOne(?int $now = null): JobRunResult
    {
        $now ??= time();
        $envelope = $this->queue->reserve($now, $this->leaseSeconds);
        if ($envelope === null) {
            return JobRunResult::Idle;
        }
        try {
            $outcome = ($this->handler)($envelope->job);
        } catch (\Throwable $failure) {
            return $this->retryOrDeadLetter($envelope, $now, $failure);
        }
        return match ($outcome) {
            JobOutcome::Complete => $this->complete($envelope),
            JobOutcome::Retry => $this->retryOrDeadLetter($envelope, $now),
            JobOutcome::DeadLetter => $this->deadLetter($envelope),
        };
    }

    private function complete(JobEnvelope $envelope): JobRunResult
    {
        $this->queue->complete($envelope);
        return JobRunResult::Completed;
    }

    private function retryOrDeadLetter(
        JobEnvelope $envelope,
        int $now,
        ?\Throwable $failure = null,
    ): JobRunResult {
        if ($envelope->attempts >= $envelope->maximumAttempts) {
            return $this->deadLetter($envelope, $failure);
        }
        $this->queue->release($envelope, $now + $this->retryDelaySeconds, $failure);
        return JobRunResult::Retried;
    }

    private function deadLetter(JobEnvelope $envelope, ?\Throwable $failure = null): JobRunResult
    {
        $this->queue->deadLetter($envelope, $failure);
        return JobRunResult::DeadLettered;
    }
}
