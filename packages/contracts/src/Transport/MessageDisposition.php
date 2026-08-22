<?php

declare(strict_types=1);

namespace Pam\Contracts\Transport;

enum MessageDisposition: int
{
    case Acknowledge = 1;
    case Retry = 2;
    case Reject = 3;
}
