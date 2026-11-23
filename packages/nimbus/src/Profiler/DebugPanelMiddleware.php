<?php

declare(strict_types=1);

namespace Nimbus\Profiler;

use Nimbus\Auth\AuthContext;
use Nimbus\Auth\Authenticator;
use Nimbus\Auth\Gate;
use Nimbus\Auth\Principal;
use Nimbus\Container\Container;
use Nimbus\Http\HttpException;
use Nimbus\Http\ProblemCode;
use Pam\Contracts\Http\MiddlewareInterface;
use Pam\Contracts\Http\RequestHandlerInterface;
use Pam\Http\Request;
use Pam\Http\Response;

/**
 * @internal Requires an authenticated principal with the `viewDebugPanel` ability. Like
 * the admin panel, the debug panel stays closed until the application defines the
 * ability (or a Gate before() hook decides).
 */
final readonly class DebugPanelMiddleware implements MiddlewareInterface
{
    public function __construct(private Container $container)
    {
    }

    public function process(Request $request, Response $response, RequestHandlerInterface $next): Response
    {
        $authenticator = $this->container->has(Authenticator::class) ? $this->container->get(Authenticator::class) : null;
        if (!$authenticator instanceof Authenticator) {
            throw new HttpException(403, ProblemCode::Forbidden, 'The debug panel requires a bound Nimbus\\Auth\\Authenticator.');
        }
        $principal = $authenticator->authenticate($request)
            ?? throw new HttpException(401, ProblemCode::Unauthenticated, 'Authentication is required.');
        $this->container
            ->scopedInstance(Principal::class, $principal)
            ->scopedInstance(AuthContext::class, new AuthContext($principal));
        $gate = $this->container->get(Gate::class);
        if (!$gate instanceof Gate
            || (!$gate->has('viewDebugPanel') && !$gate->hasBeforeHooks())
            || !$gate->check($principal, 'viewDebugPanel')
        ) {
            throw new HttpException(403, ProblemCode::Forbidden, 'This action is not authorized.');
        }
        return $next->handle($request, $response)
            ->header('cache-control', 'no-store, private')
            ->header('x-frame-options', 'DENY')
            ->header('x-content-type-options', 'nosniff')
            ->header('content-security-policy', "default-src 'none'; style-src 'unsafe-inline'; frame-ancestors 'none'; base-uri 'none'");
    }
}
