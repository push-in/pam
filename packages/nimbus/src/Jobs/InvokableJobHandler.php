<?php

declare(strict_types=1);

namespace Nimbus\Jobs;

use Nimbus\Container\Container;

/**
 * Runs jobs that carry their own `handle()` method. With a container, the
 * method's parameters are injected; a returned JobOutcome is honoured and
 * anything else counts as complete.
 */
final readonly class InvokableJobHandler implements JobHandler
{
    public function __construct(private ?Container $container = null)
    {
    }

    public function handle(object $job): JobOutcome
    {
        if (!method_exists($job, 'handle')) {
            throw new \LogicException($job::class . ' has no handle() method.');
        }
        $result = $this->container !== null ? $this->container->call([$job, 'handle']) : $job->handle();
        return $result instanceof JobOutcome ? $result : JobOutcome::Complete;
    }
}
