<?php

declare(strict_types=1);

namespace Nimbus\Jobs;

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
        private ?RetryPolicy $retryPolicy = null,
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

    /**
     * Runs jobs until the queue is idle, `$maximumJobs` ran or `$maximumSeconds`
     * passed, so a tick of a PAM task worker stays short.
     *
     * @param (\Closure(): int)|null $clock Seconds since the epoch; defaults to time().
     * @return int<0, max> Jobs that ran (completed, retried or dead-lettered).
     */
    public function work(int $maximumJobs = 100, float $maximumSeconds = 1.0, ?\Closure $clock = null): int
    {
        if ($maximumJobs < 1 || $maximumSeconds <= 0.0) {
            throw new \InvalidArgumentException('Worker limits must be positive.');
        }
        $deadline = microtime(true) + $maximumSeconds;
        $ran = 0;
        while ($ran < $maximumJobs && microtime(true) < $deadline) {
            if ($this->runOne($clock === null ? null : $clock()) === JobRunResult::Idle) {
                break;
            }
            ++$ran;
        }
        return $ran;
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
        $delay = $this->retryPolicy?->delaySeconds(max(1, $envelope->attempts)) ?? $this->retryDelaySeconds;
        $this->queue->release($envelope, $now + $delay, $failure);
        return JobRunResult::Retried;
    }

    private function deadLetter(JobEnvelope $envelope, ?\Throwable $failure = null): JobRunResult
    {
        $this->queue->deadLetter($envelope, $failure);
        return JobRunResult::DeadLettered;
    }
}
