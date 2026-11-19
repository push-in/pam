<?php

declare(strict_types=1);

namespace Nimbus\Notifications\Database;

/**
 * One stored notification.
 */
final readonly class DatabaseNotification implements \JsonSerializable
{
    /** @param array<array-key, mixed> $data */
    public function __construct(
        public string $id,
        public string $type,
        public array $data,
        public ?\DateTimeImmutable $readAt,
        public \DateTimeImmutable $createdAt,
    ) {
    }

    public function read(): bool
    {
        return $this->readAt !== null;
    }

    /** @return array{id: string, type: string, data: array<array-key, mixed>, read_at: ?string, created_at: string} */
    public function jsonSerialize(): array
    {
        return [
            'id' => $this->id,
            'type' => $this->type,
            'data' => $this->data,
            'read_at' => $this->readAt?->format(\DateTimeInterface::RFC3339),
            'created_at' => $this->createdAt->format(\DateTimeInterface::RFC3339),
        ];
    }
}
