<?php

declare(strict_types=1);

namespace App\Models;

use Arcano\Annotated\Annotation\Column;
use Arcano\Annotated\Annotation\Entity;
use Nimbus\Auth\AuthUser;
use Nimbus\Auth\Principal;
use Nimbus\Notifications\RoutesNotifications;

#[Entity(role: 'user', table: 'users')]
class User implements AuthUser, Principal
{
    use RoutesNotifications;

    #[Column(type: 'primary')]
    public ?int $id = null;

    #[Column(type: 'string(120)')]
    public string $name = '';

    #[Column(type: 'string(254)')]
    public string $email = '';

    #[Column(type: 'string(255)')]
    public string $password = '';

    #[Column(type: 'datetime', name: 'email_verified_at', nullable: true)]
    public ?\DateTimeImmutable $emailVerifiedAt = null;

    #[Column(type: 'datetime', name: 'created_at')]
    public \DateTimeImmutable $createdAt;

    public function identifier(): string
    {
        return (string) ($this->id ?? throw new \LogicException('The user is not saved yet.'));
    }

    public function can(string $ability): bool
    {
        return false;
    }

    public function authIdentifier(): string
    {
        return $this->identifier();
    }

    public function hasVerifiedEmail(): bool
    {
        return $this->emailVerifiedAt !== null;
    }
}
