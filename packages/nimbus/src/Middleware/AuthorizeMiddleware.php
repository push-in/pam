<?php

declare(strict_types=1);

namespace Nimbus\Middleware;

use Nimbus\Auth\AuthContext;
use Nimbus\Container\Container;
use Nimbus\Http\HttpException;
use Nimbus\Http\ProblemCode;
use Pam\Contracts\Http\MiddlewareInterface;
use Pam\Contracts\Http\RequestHandlerInterface;
use Pam\Http\Request;
use Pam\Http\Response;

final readonly class AuthorizeMiddleware implements MiddlewareInterface
{
    public function __construct(
        private Container $container,
        private string $ability,
    ) {
        if ($ability === '') {
            throw new \InvalidArgumentException('Authorization ability cannot be empty.');
        }
    }

    public function process(Request $request, Response $response, RequestHandlerInterface $next): Response
    {
        $auth = $this->container->get(AuthContext::class);
        if (!$auth instanceof AuthContext || !$auth->principal->can($this->ability)) {
            throw new HttpException(403, ProblemCode::Forbidden, 'This action is not authorized.');
        }
        return $next->handle($request, $response);
    }
}
