<?php

declare(strict_types=1);

namespace Pam\Api\Jobs;

final class MemoryJobDispatcher implements JobDispatcher
{
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
        if ($delaySeconds < 0 || count($this->queue) >= $this->maximumQueuedJobs) {
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

    /** @return list<JobEnvelope> */
    public function pending(): array
    {
        return $this->queue;
    }
}

