<?php

declare(strict_types=1);

namespace Pam\Api\Auth;

interface TokenRevocationStore
{
    public function revoke(string $tokenIdentifier, int $expiresAt): void;

    public function isRevoked(string $tokenIdentifier, int $now): bool;
}
