<?php

declare(strict_types=1);

namespace Nimbus\Middleware;

use Nimbus\Auth\AuthContext;
use Nimbus\Auth\TokenPrincipal;
use Nimbus\Container\Container;
use Nimbus\Http\Cookie;
use Nimbus\Http\FormData;
use Nimbus\Http\HttpException;
use Nimbus\Http\ProblemCode;
use Nimbus\Session\Session;
use Pam\Contracts\Http\MiddlewareInterface;
use Pam\Contracts\Http\RequestHandlerInterface;
use Pam\Http\Request;
use Pam\Http\Response;

/**
 * Rejects state-changing requests (419) whose `_token` field, `X-CSRF-TOKEN` or
 * `X-XSRF-TOKEN` header does not match the session token. GET, HEAD and OPTIONS pass, as
 * do requests with a bearer token (not ambient credentials) and paths in `$except`. A request
 * that skipped the check for its bearer token can then only be authenticated by that token:
 * the session authenticator ignores its session cookie, which a cross-site request carries
 * (`/webhooks/*` matches below `/webhooks/`, `/webhooks*` also `/webhooks` itself). Runs after StartSessionMiddleware; when the session has
 * a token, its value is sent in the `XSRF-TOKEN` cookie for JavaScript clients.
 */
final readonly class VerifyCsrfTokenMiddleware implements MiddlewareInterface
{
    /** Scoped flag: the CSRF check was skipped for a bearer token, so the session cannot authenticate. */
    public const string BEARER_ONLY = 'nimbus.csrf.bearer_only';

    /** @param list<string> $except */
    public function __construct(
        private Container $container,
        private array $except = [],
        private bool $xsrfCookie = true,
        private bool $secureCookie = true,
    ) {
    }

    public function process(Request $request, Response $response, RequestHandlerInterface $next): Response
    {
        $session = $this->container->scopedValue(Session::class);
        if (!$session instanceof Session) {
            throw new \LogicException('VerifyCsrfTokenMiddleware must run after StartSessionMiddleware.');
        }
        if (!$this->skips($request) && !$this->matches($request, $session)) {
            throw new HttpException(419, ProblemCode::CsrfTokenMismatch, 'The CSRF token is missing or invalid.');
        }
        $response = $next->handle($request, $response);
        if ($this->xsrfCookie && $session->has('_token')) {
            $response->addHeader('set-cookie', (new Cookie('XSRF-TOKEN', $session->token(), 7_200, secure: $this->secureCookie, httpOnly: false))->toHeader());
        }
        return $response;
    }

    private function skips(Request $request): bool
    {
        if (in_array($request->method, ['GET', 'HEAD', 'OPTIONS'], true)) {
            return true;
        }
        if (preg_match('/^\s*bearer\s+\S/i', $request->getHeader('authorization') ?? '') === 1) {
            $auth = $this->container->scopedValue(AuthContext::class);
            if ($auth instanceof AuthContext && !$auth->principal instanceof TokenPrincipal) {
                // Already authenticated by something other than a token (the session).
                return false;
            }
            $this->container->scopedInstance(self::BEARER_ONLY, true);
            return true;
        }
        foreach ($this->except as $pattern) {
            if ($pattern === $request->path || (str_ends_with($pattern, '*') && self::underPrefix($request->path, substr($pattern, 0, -1)))) {
                return true;
            }
        }
        return false;
    }

    /**
     * `/webhooks/*` matches below `/webhooks/`; `/webhooks*` matches `/webhooks` and below it,
     * never a sibling such as `/webhooksX`.
     */
    private static function underPrefix(string $path, string $prefix): bool
    {
        if ($prefix === '' || str_ends_with($prefix, '/')) {
            return str_starts_with($path, $prefix);
        }
        return $path === $prefix || str_starts_with($path, $prefix . '/');
    }

    private function matches(Request $request, Session $session): bool
    {
        $expected = $session->get('_token');
        if (!is_string($expected) || $expected === '') {
            return false;
        }
        $given = $request->getHeader('x-csrf-token')
            ?? $request->getHeader('x-xsrf-token')
            ?? FormData::field($request, '_token');
        return is_string($given) && hash_equals($expected, $given);
    }
}
