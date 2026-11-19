<?php

declare(strict_types=1);

namespace Nimbus\Auth;

use Pam\Http\Request;

interface Authenticator
{
    public function authenticate(Request $request): ?Principal;
}

