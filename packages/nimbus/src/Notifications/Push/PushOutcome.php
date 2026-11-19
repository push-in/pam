<?php

declare(strict_types=1);

namespace Nimbus\Notifications\Push;

enum PushOutcome: int
{
    case Delivered = 1;
    /** The device token is unregistered or invalid; the application should forget it. */
    case InvalidToken = 2;
}
