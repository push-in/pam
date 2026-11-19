<?php

declare(strict_types=1);

namespace Nimbus\Auth;

use Pam\Http\Request;

/** Authenticates `Authorization: Bearer {JWT}` with a `JwtCodec`, honouring a revocation store. */
final readonly class JwtAuthenticator implements Authenticator
{
    private BearerTokenAuthenticator $bearer;

    public function __construct(JwtCodec $codec, ?TokenRevocationStore $revocations = null)
    {
        $this->bearer = new BearerTokenAuthenticator($codec, $revocations);
    }

    public function authenticate(Request $request): ?Principal
    {
        return $this->bearer->authenticate($request);
    }
}
