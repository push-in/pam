<?php

declare(strict_types=1);

namespace Nimbus\Auth;

final readonly class AuthContext
{
    public function __construct(public Principal $principal)
    {
    }
}

