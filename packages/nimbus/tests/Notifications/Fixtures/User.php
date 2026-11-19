<?php

declare(strict_types=1);

namespace Nimbus\Tests\Notifications\Fixtures;

use Nimbus\Notifications\Notifiable;
use Nimbus\Notifications\Notification;
use Nimbus\Notifications\RoutesNotifications;

final class User implements Notifiable
{
    use RoutesNotifications;

    /** @param list<string> $devices */
    public function __construct(
        public int $id,
        public ?string $email = null,
        public array $devices = [],
        public ?string $webhookUrl = null,
    ) {
    }

    /** @return list<string> */
    public function routeNotificationForPush(Notification $notification): array
    {
        return $this->devices;
    }

    public function routeNotificationForWebhook(Notification $notification): ?string
    {
        return $this->webhookUrl;
    }
}
