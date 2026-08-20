<?php

declare(strict_types=1);

namespace Pam\Api\Health;

enum HealthState: int
{
    case Healthy = 1;
    case Degraded = 2;
    case Unhealthy = 3;
}

