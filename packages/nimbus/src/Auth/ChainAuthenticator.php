<?php

declare(strict_types=1);

namespace Nimbus\Auth;

use Pam\Http\Request;

/** Tries each authenticator in order, e.g. JWT access tokens first and personal access tokens next. */
final readonly class ChainAuthenticator implements Authenticator
{
    /** @var list<Authenticator> */
    private array $authenticators;

    public function __construct(Authenticator ...$authenticators)
    {
        $this->authenticators = array_values($authenticators);
    }

    public function authenticate(Request $request): ?Principal
    {
        foreach ($this->authenticators as $authenticator) {
            $principal = $authenticator->authenticate($request);
            if ($principal !== null) {
                return $principal;
            }
        }
        return null;
    }
}
