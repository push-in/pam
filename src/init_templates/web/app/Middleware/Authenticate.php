<?php

declare(strict_types=1);

namespace App\Middleware;

use Nimbus\Auth\AuthContext;
use Nimbus\Auth\Principal;
use Nimbus\Auth\SessionAuthenticator;
use Nimbus\Container\Container;
use Nimbus\Http\Redirect;
use Pam\Contracts\Http\MiddlewareInterface;
use Pam\Contracts\Http\RequestHandlerInterface;
use Pam\Http\Request;
use Pam\Http\Response;

/** Pages for signed-in users: guests are sent to the login screen. */
final readonly class Authenticate implements MiddlewareInterface
{
    public function __construct(
        private SessionAuthenticator $sessions,
        private Container $container,
    ) {
    }

    public function process(Request $request, Response $response, RequestHandlerInterface $next): Response
    {
        $principal = $this->sessions->authenticate($request);
        if ($principal === null) {
            return Redirect::to('/login')->toResponse($request, $response);
        }
        $this->container
            ->scopedInstance(Principal::class, $principal)
            ->scopedInstance(AuthContext::class, new AuthContext($principal));
        return $next->handle($request, $response);
    }
}
