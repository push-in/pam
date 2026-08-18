<?php

declare(strict_types=1);

namespace Pam\Contracts\Transport;

enum TransportEventCode: int
{
    case Starting = 1;
    case Ready = 2;
    case MessageReceived = 3;
    case MessageAcknowledged = 4;
    case RetryScheduled = 5;
    case MessageRejected = 6;
    case MessagePublished = 7;
    case Failed = 8;
    case Stopped = 9;
}
