<?php

declare(strict_types=1);

namespace Nimbus\Jobs;

interface JobHandler
{
    public function handle(object $job): JobOutcome;
}
