<?php

declare(strict_types=1);

namespace Pam\Api\Auth;

final readonly class AuthContext
{
    public function __construct(public Principal $principal)
    {
    }
}

