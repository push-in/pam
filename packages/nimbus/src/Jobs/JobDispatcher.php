<?php

declare(strict_types=1);

namespace Nimbus\Jobs;

interface JobDispatcher
{
    public function dispatch(object $job, int $maximumAttempts = 3, int $delaySeconds = 0): JobEnvelope;
}

