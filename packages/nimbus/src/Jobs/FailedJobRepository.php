<?php

declare(strict_types=1);

namespace Nimbus\Jobs;

/** The failed-jobs store of a persistent queue. */
interface FailedJobRepository
{
    /** @return list<FailedJob> Most recent first. */
    public function failed(int $limit = 100): array;

    /** Puts a failed job back on its queue with a fresh attempt budget. */
    public function retryFailed(string $id): bool;

    public function forgetFailed(string $id): bool;
}
