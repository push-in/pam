<?php

declare(strict_types=1);

namespace Pam\Api;

use Pam\Api\Container\Container;
use Pam\Contracts\Http\MiddlewareInterface;
use Pam\Contracts\Http\RequestHandlerInterface;
use Pam\Http\Request;
use Pam\Http\Response;

final readonly class ContainerMiddleware implements MiddlewareInterface
{
    /** @param class-string $middleware */
    public function __construct(
        private Container $container,
        private string $middleware,
    ) {
        if (!is_a($middleware, MiddlewareInterface::class, true)) {
            throw new \InvalidArgumentException("Middleware {$middleware} must implement MiddlewareInterface.");
        }
    }

    public function process(Request $request, Response $response, RequestHandlerInterface $next): Response
    {
        $middleware = $this->container->get($this->middleware);
        if (!$middleware instanceof MiddlewareInterface) {
            throw new \UnexpectedValueException("Container entry {$this->middleware} is not middleware.");
        }
        return $middleware->process($request, $response, $next);
    }
}

