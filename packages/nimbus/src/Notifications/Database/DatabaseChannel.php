<?php

declare(strict_types=1);

namespace Nimbus\Notifications\Database;

use Nimbus\Database\ArcanoDatabaseProvider;
use Nimbus\Notifications\Notifiable;
use Nimbus\Notifications\Notification;
use Nimbus\Notifications\NotificationChannel;
use Nimbus\Notifications\NotificationDeliveryException;

/**
 * Stores `toDatabase()` in the notifications table (see database/migrations), unread.
 * Retried deliveries are idempotent: the notification id is the primary key.
 */
final readonly class DatabaseChannel implements NotificationChannel
{
    public function __construct(
        private ArcanoDatabaseProvider $databases,
        private string $table = 'notifications',
        private ?string $connection = null,
    ) {
    }

    public function prepare(Notifiable $notifiable, Notification $notification, string $id): ?array
    {
        if ($notifiable->notifiableKey() === '') {
            return null;
        }
        return [
            'id' => $id,
            'type' => $notification->type(),
            'notifiable_type' => $notifiable->notifiableType(),
            'notifiable_id' => $notifiable->notifiableKey(),
            'data' => $notification->toDatabase($notifiable),
            'created_at' => gmdate('Y-m-d H:i:s'),
        ];
    }

    public function deliver(array $payload): void
    {
        foreach (['id', 'type', 'notifiable_type', 'notifiable_id', 'created_at'] as $key) {
            if (!is_string($payload[$key] ?? null)) {
                throw NotificationDeliveryException::permanent('Database notification payload is invalid.');
            }
        }
        $data = $payload['data'] ?? [];
        /** @var array{id: string, type: string, notifiable_type: string, notifiable_id: string, created_at: string} $payload */
        $database = $this->databases->database($this->connection);
        if ($database->select()->from($this->table)->where('id', $payload['id'])->count() > 0) {
            return;
        }
        $database->insert($this->table)->values([
            'id' => $payload['id'],
            'type' => $payload['type'],
            'notifiable_type' => $payload['notifiable_type'],
            'notifiable_id' => $payload['notifiable_id'],
            'data' => json_encode($data, JSON_THROW_ON_ERROR | JSON_UNESCAPED_SLASHES | JSON_UNESCAPED_UNICODE),
            'read_at' => null,
            'created_at' => new \DateTimeImmutable($payload['created_at'], new \DateTimeZone('UTC')),
        ])->run();
    }
}
