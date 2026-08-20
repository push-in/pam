<?php

declare(strict_types=1);

namespace Pam\Api\Jobs;

final class JobEnvelope
{
    public JobState $state = JobState::Pending;
    public int $attempts = 0;

    public function __construct(
        public readonly string $id,
        public readonly object $job,
        public readonly int $maximumAttempts = 3,
        public readonly int $availableAt = 0,
    ) {
        if ($id === '' || $maximumAttempts < 1) {
            throw new \InvalidArgumentException('Job envelope configuration is invalid.');
        }
    }
}

