<?php

declare(strict_types=1);

namespace Nimbus\Auth;

/** Verifies a signed access token, returning null for anything invalid or expired. */
interface AccessTokenVerifier
{
    public function verify(string $token): ?TokenPrincipal;
}
