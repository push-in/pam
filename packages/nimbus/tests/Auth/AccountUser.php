<?php

declare(strict_types=1);

namespace Nimbus\Tests\Auth;

use Nimbus\Auth\AuthUser;
use Nimbus\Notifications\RoutesNotifications;

final class AccountUser implements AuthUser
{
    use RoutesNotifications;

    public function __construct(
        public string $id,
        public string $email,
        public string $passwordHash = '',
        public bool $verified = false,
    ) {
    }

    public function authIdentifier(): string
    {
        return $this->id;
    }

    public function hasVerifiedEmail(): bool
    {
        return $this->verified;
    }
}
