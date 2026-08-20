<?php

declare(strict_types=1);

namespace Pam\Api\Auth;

use Pam\Http\Request;

interface Authenticator
{
    public function authenticate(Request $request): ?Principal;
}

