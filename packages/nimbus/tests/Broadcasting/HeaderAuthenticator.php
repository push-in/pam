<?php

declare(strict_types=1);

namespace Nimbus\Tests\Broadcasting;

use Nimbus\Auth\Authenticator;
use Nimbus\Auth\Principal;
use Pam\Http\Request;

/** Authenticates the `x-user` header as a principal with that identifier. */
final class HeaderAuthenticator implements Authenticator
{
    public function authenticate(Request $request): ?Principal
    {
        $id = $request->getHeader('x-user');
        return $id === null ? null : new class ($id) implements Principal {
            public function __construct(private readonly string $id)
            {
            }

            public function identifier(): string
            {
                return $this->id;
            }

            public function can(string $ability): bool
            {
                return false;
            }
        };
    }
}
