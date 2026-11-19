<?php

declare(strict_types=1);

namespace Nimbus\Jobs;

/** Decides how long a failed job waits before its next attempt. */
interface RetryPolicy
{
    /**
     * @param positive-int $attempt The attempt that just failed, starting at 1.
     * @return int<0, max> Seconds until the job becomes available again.
     */
    public function delaySeconds(int $attempt): int;
}
