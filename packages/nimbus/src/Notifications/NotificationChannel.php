<?php

declare(strict_types=1);

namespace Nimbus\Notifications;

/**
 * A delivery channel. `prepare()` renders a JSON-safe payload so queued deliveries
 * never need the notifiable again; `deliver()` sends one payload.
 */
interface NotificationChannel
{
    /** @return array<string, mixed>|null Null skips the channel (for example, no route). */
    public function prepare(Notifiable $notifiable, Notification $notification, string $id): ?array;

    /**
     * @param array<string, mixed> $payload
     * @throws NotificationDeliveryException
     */
    public function deliver(array $payload): void;
}
