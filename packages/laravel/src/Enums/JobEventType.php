<?php

declare(strict_types=1);

namespace Pam\Laravel\Enums;

enum JobEventType: int
{
    case Processing = 1;
    case Processed = 2;
    case Failed = 3;
}
