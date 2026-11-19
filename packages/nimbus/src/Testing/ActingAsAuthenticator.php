<?php

declare(strict_types=1);

namespace Nimbus\Testing;

use Nimbus\Auth\Authenticator;
use Nimbus\Auth\Principal;
use Pam\Http\Request;

/** Authenticates every request as the same principal; bound by `actingAs()` in tests. */
final readonly class ActingAsAuthenticator implements Authenticator
{
    public function __construct(public Principal $principal)
    {
    }

    public function authenticate(Request $request): Principal
    {
        return $this->principal;
    }
}
