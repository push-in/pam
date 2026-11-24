<?php

declare(strict_types=1);

namespace Nimbus\Sentry;

use Nimbus\Jobs\JobHandler;
use Nimbus\Jobs\JobOutcome;

/**
 * Reports a job that throws, tagged with its class, and rethrows so the
 * worker still retries or dead-letters it.
 */
final readonly class SentryJobHandler implements JobHandler
{
    /** @var \Closure(object): JobOutcome */
    private \Closure $handler;

    /** @param JobHandler|callable(object): JobOutcome $handler */
    public function __construct(JobHandler|callable $handler, private SentryReporter $reporter)
    {
        $this->handler = $handler instanceof JobHandler ? $handler->handle(...) : \Closure::fromCallable($handler);
    }

    public function handle(object $job): JobOutcome
    {
        try {
            return ($this->handler)($job);
        } catch (\Throwable $error) {
            $this->reporter->capture($error, ['job' => $job::class]);
            throw $error;
        }
    }
}
