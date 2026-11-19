<?php

declare(strict_types=1);

namespace Nimbus\Jobs;

enum JobState: int
{
    case Pending = 1;
    case Processing = 2;
    case Completed = 3;
    case Failed = 4;
    case DeadLetter = 5;
}

