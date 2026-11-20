<?php

declare(strict_types=1);

namespace Nimbus\Auth;

use Nimbus\Container\Container;
use Nimbus\Middleware\VerifyCsrfTokenMiddleware;
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

    private const string LOGGED_IN_AT = '_auth_at';

    /** @var \Closure(string): ?Principal */
    private \Closure $resolver;

    /** @var \Closure(): int */
    private \Closure $clock;

    /**
     * @param \Closure(string): ?Principal $resolver Finds the principal of a stored identifier.
     * @param SessionRevocations|null $revocations Rejects sessions that logged in before the user's
     *     last revocation (password reset, "sign out everywhere").
     * @param (\Closure(): int)|null $clock
     */
    public function __construct(
        private Container $container,
        \Closure $resolver,
        private ?SessionRevocations $revocations = null,
        ?\Closure $clock = null,
    ) {
        $this->resolver = $resolver;
        $this->clock = $clock ?? static fn (): int => time();
    }

    public function authenticate(Request $request): ?Principal
    {
        if ($this->container->scopedValue(VerifyCsrfTokenMiddleware::BEARER_ONLY) === true) {
            // The CSRF check was skipped because of a bearer token: only that token may
            // authenticate this request, not the session cookie it carries.
            return null;
        }
        $session = $this->session();
        $identifier = $session?->get(self::KEY);
        if (!is_string($identifier) || $identifier === '' || $session === null) {
            return null;
        }
        if ($this->revocations !== null) {
            $revokedAt = $this->revocations->revokedAt($identifier);
            $loggedInAt = $session->get(self::LOGGED_IN_AT);
            if ($revokedAt !== null && (!is_int($loggedInAt) || $loggedInAt <= $revokedAt)) {
                $session->invalidate();
                return null;
            }
        }
        return ($this->resolver)($identifier);
    }

    /** Logs the principal in for this session and the following requests. */
    public function login(Principal $principal): void
    {
        $session = $this->session() ?? throw new \LogicException('Session login requires StartSessionMiddleware.');
        $session->regenerate()->regenerateToken();
        $session->put(self::KEY, $principal->identifier());
        $session->put(self::LOGGED_IN_AT, ($this->clock)());
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
