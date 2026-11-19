<?php

declare(strict_types=1);

namespace Nimbus\Middleware;

use Nimbus\Auth\AuthContext;
use Nimbus\Auth\Authenticator;
use Nimbus\Auth\Principal;
use Nimbus\Container\Container;
use Nimbus\Http\HttpException;
use Nimbus\Http\ProblemCode;
use Pam\Contracts\Http\MiddlewareInterface;
use Pam\Contracts\Http\RequestHandlerInterface;
use Pam\Http\Request;
use Pam\Http\Response;

final readonly class AuthenticateMiddleware implements MiddlewareInterface
{
    public function __construct(
        private Authenticator $authenticator,
        private Container $container,
    ) {
    }

    public function process(Request $request, Response $response, RequestHandlerInterface $next): Response
    {
        $principal = $this->authenticator->authenticate($request);
        if ($principal === null) {
            throw new HttpException(401, ProblemCode::Unauthenticated, 'Authentication is required.');
        }
        $this->container
            ->scopedInstance(Principal::class, $principal)
            ->scopedInstance(AuthContext::class, new AuthContext($principal));
        return $next->handle($request, $response);
    }
}

