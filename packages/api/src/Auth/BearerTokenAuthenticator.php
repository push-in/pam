<?php

declare(strict_types=1);

namespace Pam\Api\Auth;

use Pam\Http\Request;

final readonly class BearerTokenAuthenticator implements Authenticator
{
    public function __construct(private HmacTokenCodec $tokens)
    {
    }

    public function authenticate(Request $request): ?Principal
    {
        $authorization = $request->getHeader('authorization');
        if ($authorization === null || preg_match('/^Bearer ([A-Za-z0-9._-]+)$/D', $authorization, $matches) !== 1) {
            return null;
        }
        return $this->tokens->verify($matches[1]);
    }
}
