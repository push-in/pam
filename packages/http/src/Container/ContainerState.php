<?php

declare(strict_types=1);

namespace Pam\Api\Container;

enum ContainerState: int
{
    case Idle = 1;
    case RequestActive = 2;
}

