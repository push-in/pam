<?php

declare(strict_types=1);

namespace Nimbus\Auth;

/** A stored personal access token; the plain token is never kept. */
final readonly class PersonalAccessToken
{
    /** @param list<string> $abilities */
    public function __construct(
        public string $id,
        public string $subject,
        public string $name,
        public array $abilities,
        public ?int $lastUsedAt,
        public ?int $expiresAt,
        public int $createdAt,
    ) {
    }

    public function can(string $ability): bool
    {
        return Abilities::allows($this->abilities, $ability);
    }

    public function expired(int $now): bool
    {
        return $this->expiresAt !== null && $this->expiresAt <= $now;
    }

    /** @return array{id: string, name: string, abilities: list<string>, last_used_at: ?int, expires_at: ?int, created_at: int} */
    public function toArray(): array
    {
        return [
            'id' => $this->id,
            'name' => $this->name,
            'abilities' => $this->abilities,
            'last_used_at' => $this->lastUsedAt,
            'expires_at' => $this->expiresAt,
            'created_at' => $this->createdAt,
        ];
    }
}
