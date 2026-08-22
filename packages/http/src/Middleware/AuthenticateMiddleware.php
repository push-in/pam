<?php

declare(strict_types=1);

namespace Pam\Api\Middleware;

use Pam\Api\Auth\AuthContext;
use Pam\Api\Auth\Authenticator;
use Pam\Api\Auth\Principal;
use Pam\Api\Container\Container;
use Pam\Api\Http\HttpException;
use Pam\Api\Http\ProblemCode;
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

