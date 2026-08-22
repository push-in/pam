<?php

declare(strict_types=1);

namespace Pam\Api\Jobs;

interface JobHandler
{
    public function handle(object $job): JobOutcome;
}
