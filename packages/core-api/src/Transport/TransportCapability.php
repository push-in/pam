<?php

declare(strict_types=1);

namespace Pam\Contracts\Transport;

enum TransportCapability: int
{
    case Publish = 1;
    case Consume = 2;
    case DelayedRetry = 3;
    case DeadLetter = 4;
    case OrderedDelivery = 5;
}
