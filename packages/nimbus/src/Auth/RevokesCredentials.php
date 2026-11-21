<?php

declare(strict_types=1);

namespace Nimbus\Auth;

/** Something that can sign a user out everywhere: their refresh tokens, personal tokens or sessions. */
interface RevokesCredentials
{
    /** Revokes every credential of the subject; returns how many were revoked (when known). */
    public function revokeAll(string $subject): int;
}
