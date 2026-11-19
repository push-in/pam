<?php

declare(strict_types=1);

namespace Nimbus\Health;

enum HealthState: int
{
    case Healthy = 1;
    case Degraded = 2;
    case Unhealthy = 3;
}

