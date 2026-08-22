<?php

declare(strict_types=1);

namespace Pam\Api\Resilience;

enum CircuitState: int
{
    case Closed = 1;
    case Open = 2;
    case HalfOpen = 3;
}

