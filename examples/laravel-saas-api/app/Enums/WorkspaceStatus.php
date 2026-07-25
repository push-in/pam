<?php

declare(strict_types=1);

namespace App\Enums;

enum WorkspaceStatus: int
{
    case Trial = 1;
    case Active = 2;
    case Suspended = 3;
}
