<?php

declare(strict_types=1);

namespace Nimbus\Scheduling;

/**
 * A lock shared by every worker that runs the scheduler. The scheduler claims
 * each (task, minute) pair through it, so a task runs once per due minute no
 * matter how many workers or servers tick.
 */
interface SchedulerLock
{
    /** Takes `$key` for `$seconds` unless another holder has it. */
    public function acquire(string $key, int $seconds): bool;

    /** Frees `$key` if this instance holds it. */
    public function release(string $key): void;
}
