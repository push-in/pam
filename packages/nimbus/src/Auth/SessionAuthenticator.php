<?php

declare(strict_types=1);

namespace Nimbus\Auth;

use Nimbus\Container\Container;
use Nimbus\Session\Session;
use Pam\Http\Request;

/**
 * Web login through the session: `login()` regenerates the session id (no fixation) and
 * remembers the principal's identifier; `authenticate()` turns it back into a principal
 * with the resolver. Combine it with bearer tokens through ChainAuthenticator.
 */
final readonly class SessionAuthenticator implements Authenticator
{
    private const string KEY = '_auth_id';

    /** @var \Closure(string): ?Principal */
    private \Closure $resolver;

    /** @param \Closure(string): ?Principal $resolver Finds the principal of a stored identifier. */
    public function __construct(
        private Container $container,
        \Closure $resolver,
    ) {
        $this->resolver = $resolver;
    }

    public function authenticate(Request $request): ?Principal
    {
        $identifier = $this->session()?->get(self::KEY);
        return is_string($identifier) && $identifier !== '' ? ($this->resolver)($identifier) : null;
    }

    /** Logs the principal in for this session and the following requests. */
    public function login(Principal $principal): void
    {
        $session = $this->session() ?? throw new \LogicException('Session login requires StartSessionMiddleware.');
        $session->regenerate()->regenerateToken();
        $session->put(self::KEY, $principal->identifier());
        $this->container
            ->scopedInstance(Principal::class, $principal)
            ->scopedInstance(AuthContext::class, new AuthContext($principal));
    }

    /** Clears the session, with a new id and CSRF token. */
    public function logout(): void
    {
        $this->session()?->invalidate();
    }

    private function session(): ?Session
    {
        $session = $this->container->scopedValue(Session::class);
        return $session instanceof Session ? $session : null;
    }
}
