<?php

declare(strict_types=1);

namespace Pam\Api\Jobs;

enum JobOutcome: int
{
    case Complete = 1;
    case Retry = 2;
    case DeadLetter = 3;
}
