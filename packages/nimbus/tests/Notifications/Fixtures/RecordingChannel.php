<?php

declare(strict_types=1);

namespace Nimbus\Tests\Notifications\Fixtures;

use Nimbus\Notifications\Notifiable;
use Nimbus\Notifications\Notification;
use Nimbus\Notifications\NotificationChannel;
use Nimbus\Notifications\NotificationDeliveryException;

final class RecordingChannel implements NotificationChannel
{
    /** @var list<array<string, mixed>> */
    public array $delivered = [];

    /** @var list<NotificationDeliveryException> */
    public array $failures = [];

    public function prepare(Notifiable $notifiable, Notification $notification, string $id): array
    {
        return ['id' => $id, 'key' => $notifiable->notifiableKey(), 'type' => $notification->type()];
    }

    public function deliver(array $payload): void
    {
        $failure = array_shift($this->failures);
        if ($failure !== null) {
            throw $failure;
        }
        $this->delivered[] = $payload;
    }
}
