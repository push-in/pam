<?php

declare(strict_types=1);

namespace Nimbus\Tests\Auth;

use Nimbus\Auth\AuthUser;
use Nimbus\Auth\AuthUserProvider;

final class AccountUsers implements AuthUserProvider
{
    /** @var array<string, AccountUser> */
    public array $users = [];

    public function __construct(AccountUser ...$users)
    {
        foreach ($users as $user) {
            $this->users[$user->id] = $user;
        }
    }

    public function findById(string $identifier): ?AuthUser
    {
        return $this->users[$identifier] ?? null;
    }

    public function findByEmail(string $email): ?AuthUser
    {
        foreach ($this->users as $user) {
            if (strcasecmp($user->email, $email) === 0) {
                return $user;
            }
        }
        return null;
    }

    public function updatePassword(AuthUser $user, string $passwordHash): void
    {
        $this->users[$user->authIdentifier()]->passwordHash = $passwordHash;
    }

    public function markEmailAsVerified(AuthUser $user): void
    {
        $this->users[$user->authIdentifier()]->verified = true;
    }
}
