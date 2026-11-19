<?php

declare(strict_types=1);

namespace Nimbus\Jobs;

enum JobRunResult: int
{
    case Idle = 1;
    case Completed = 2;
    case Retried = 3;
    case DeadLettered = 4;
}
