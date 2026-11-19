<?php

declare(strict_types=1);

namespace Nimbus\Auth;

/** The application's user storage, as seen by `PasswordResets` and `EmailVerification`. */
interface AuthUserProvider
{
    public function findById(string $identifier): ?AuthUser;

    public function findByEmail(string $email): ?AuthUser;

    /** Stores the new password hash (made with `Nimbus\Security\Hash`). */
    public function updatePassword(AuthUser $user, string $passwordHash): void;

    public function markEmailAsVerified(AuthUser $user): void;
}
