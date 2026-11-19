<?php

declare(strict_types=1);

namespace Nimbus\Notifications\Push;

enum PushPlatform: string
{
    case Fcm = 'fcm';
    case Apns = 'apns';
}
