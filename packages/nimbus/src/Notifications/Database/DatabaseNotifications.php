<?php

declare(strict_types=1);

namespace Nimbus\Notifications\Database;

use Arcano\Database\DatabaseInterface;
use Arcano\Database\Query\SelectQuery;
use Nimbus\Database\ArcanoDatabaseProvider;
use Nimbus\Notifications\Notifiable;

/**
 * Reads and marks the stored notifications of one notifiable. Every query is scoped to the
 * notifiable, so ids from a request cannot touch another user's notifications.
 */
final readonly class DatabaseNotifications
{
    private const int MAXIMUM_LIMIT = 200;

    public function __construct(
        private ArcanoDatabaseProvider $databases,
        private string $table = 'notifications',
        private ?string $connection = null,
    ) {
    }

    /**
     * Newest first; `$before` is the id of the last notification of the previous page.
     *
     * @return list<DatabaseNotification>
     */
    public function latest(Notifiable $notifiable, int $limit = 50, bool $unreadOnly = false, ?string $before = null): array
    {
        $query = $this->scoped($notifiable)
            ->columns(['id', 'type', 'data', 'read_at', 'created_at'])
            ->orderBy('created_at', 'DESC')
            ->orderBy('id', 'DESC')
            ->limit(max(1, min($limit, self::MAXIMUM_LIMIT)));
        if ($unreadOnly) {
            $query->where('read_at', null);
        }
        if ($before !== null) {
            $cursor = $this->find($notifiable, $before);
            if ($cursor === null) {
                return [];
            }
            $createdAt = $cursor->createdAt;
            $query->where(static function (SelectQuery $where) use ($createdAt, $before): void {
                $where->where('created_at', '<', $createdAt)
                    ->orWhere(static function (SelectQuery $same) use ($createdAt, $before): void {
                        $same->where('created_at', $createdAt)->where('id', '<', $before);
                    });
            });
        }
        return array_map(self::hydrate(...), array_values($query->fetchAll()));
    }

    public function find(Notifiable $notifiable, string $id): ?DatabaseNotification
    {
        $row = $this->scoped($notifiable)
            ->columns(['id', 'type', 'data', 'read_at', 'created_at'])
            ->where('id', $id)
            ->limit(1)
            ->fetchAll();
        return $row === [] ? null : self::hydrate(array_values($row)[0]);
    }

    public function unreadCount(Notifiable $notifiable): int
    {
        return $this->scoped($notifiable)->where('read_at', null)->count();
    }

    public function markAsRead(Notifiable $notifiable, string $id): bool
    {
        return $this->update($notifiable, ['read_at' => self::now()], $id, unreadOnly: true) > 0;
    }

    public function markAsUnread(Notifiable $notifiable, string $id): bool
    {
        return $this->update($notifiable, ['read_at' => null], $id) > 0;
    }

    /** @return int Notifications marked. */
    public function markAllAsRead(Notifiable $notifiable): int
    {
        return $this->update($notifiable, ['read_at' => self::now()], null, unreadOnly: true);
    }

    public function delete(Notifiable $notifiable, string $id): bool
    {
        return $this->database()->delete($this->table)
            ->where('notifiable_type', $notifiable->notifiableType())
            ->where('notifiable_id', $notifiable->notifiableKey())
            ->where('id', $id)
            ->run() > 0;
    }

    /** Deletes read notifications created before the cutoff; returns the deleted count. */
    public function prune(\DateTimeInterface $before): int
    {
        return $this->database()->delete($this->table)
            ->where('read_at', '!=', null)
            ->where('created_at', '<', $before)
            ->run();
    }

    /** @param array<string, mixed> $values */
    private function update(Notifiable $notifiable, array $values, ?string $id, bool $unreadOnly = false): int
    {
        $query = $this->database()->update($this->table, $values)
            ->where('notifiable_type', $notifiable->notifiableType())
            ->where('notifiable_id', $notifiable->notifiableKey());
        if ($id !== null) {
            $query->where('id', $id);
        }
        if ($unreadOnly) {
            $query->where('read_at', null);
        }
        return $query->run();
    }

    /** @return SelectQuery<mixed> */
    private function scoped(Notifiable $notifiable): SelectQuery
    {
        return $this->database()->select()->from($this->table)
            ->where('notifiable_type', $notifiable->notifiableType())
            ->where('notifiable_id', $notifiable->notifiableKey());
    }

    private function database(): DatabaseInterface
    {
        return $this->databases->database($this->connection);
    }

    private static function now(): \DateTimeImmutable
    {
        return new \DateTimeImmutable('now', new \DateTimeZone('UTC'));
    }

    private static function hydrate(mixed $row): DatabaseNotification
    {
        if (!is_array($row)) {
            throw new \UnexpectedValueException('Notification row is invalid.');
        }
        $data = json_decode(is_string($row['data'] ?? null) ? $row['data'] : '[]', true, 64, JSON_THROW_ON_ERROR);
        return new DatabaseNotification(
            self::text($row['id'] ?? null),
            self::text($row['type'] ?? null),
            is_array($data) ? $data : [],
            ($row['read_at'] ?? null) === null ? null : self::date($row['read_at']),
            self::date($row['created_at'] ?? null),
        );
    }

    private static function text(mixed $value): string
    {
        return is_scalar($value) ? (string) $value : '';
    }

    private static function date(mixed $value): \DateTimeImmutable
    {
        if ($value instanceof \DateTimeInterface) {
            return \DateTimeImmutable::createFromInterface($value);
        }
        return new \DateTimeImmutable(self::text($value), new \DateTimeZone('UTC'));
    }
}
