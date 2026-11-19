<?php

declare(strict_types=1);

namespace Nimbus\Auth;

use Pam\Http\Request;

/**
 * Authenticates `Authorization: Bearer {personal access token}`. The principal's abilities are
 * the token's, so `can('products:write')` routes check the token scope.
 */
final readonly class PersonalAccessTokenAuthenticator implements Authenticator
{
    public function __construct(private PersonalAccessTokens $tokens)
    {
    }

    public function authenticate(Request $request): ?Principal
    {
        $authorization = $request->getHeader('authorization');
        if ($authorization === null || preg_match('/^Bearer ([A-Za-z0-9._-]{1,256})$/D', $authorization, $matches) !== 1) {
            return null;
        }
        $token = $this->tokens->use($matches[1]);
        return $token === null ? null : new TokenPrincipal($token->subject, $token->abilities, 'pat:' . $token->id);
    }
}
