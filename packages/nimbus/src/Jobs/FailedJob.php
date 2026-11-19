<?php

declare(strict_types=1);

namespace Nimbus\Jobs;

/**
 * A job that exhausted its attempts or was dead-lettered. Only the exception
 * class is kept: messages may carry secrets or personal data.
 */
final readonly class FailedJob
{
    public function __construct(
        public string $id,
        public string $queue,
        public string $payload,
        public int $attempts,
        public ?string $failureClass,
        public int $failedAt,
    ) {
    }
}
