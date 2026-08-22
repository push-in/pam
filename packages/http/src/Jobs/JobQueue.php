<?php

declare(strict_types=1);

namespace Pam\Api\Jobs;

interface JobQueue extends JobDispatcher
{
    public function reserve(int $now, int $leaseSeconds): ?JobEnvelope;

    public function complete(JobEnvelope $envelope): void;

    public function release(JobEnvelope $envelope, int $availableAt, ?\Throwable $failure = null): void;

    public function deadLetter(JobEnvelope $envelope, ?\Throwable $failure = null): void;
}
