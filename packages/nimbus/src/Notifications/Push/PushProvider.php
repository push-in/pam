<?php

declare(strict_types=1);

namespace Nimbus\Notifications\Push;

use Nimbus\Notifications\NotificationDeliveryException;

/**
 * One push service (FCM, APNs).
 */
interface PushProvider
{
    /** @throws NotificationDeliveryException */
    public function send(PushMessage $message, string $token): PushOutcome;
}
