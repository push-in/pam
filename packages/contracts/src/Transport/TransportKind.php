<?php

declare(strict_types=1);

namespace Pam\Contracts\Transport;

enum TransportKind: int
{
    case Queue = 1;
    case PubSub = 2;
    case Stream = 3;
    case Rpc = 4;
}
