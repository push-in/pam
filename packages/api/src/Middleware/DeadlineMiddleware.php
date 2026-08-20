<?php

declare(strict_types=1);

namespace Pam\Api\Middleware;

use Pam\Api\Container\Container;
use Pam\Api\Runtime\Deadline;
use Pam\Contracts\Http\MiddlewareInterface;
use Pam\Contracts\Http\RequestHandlerInterface;
use Pam\Http\Request;
use Pam\Http\Response;

final readonly class DeadlineMiddleware implements MiddlewareInterface
{
    public function __construct(
        private Container $container,
        private float $seconds,
    ) {
        if ($seconds <= 0) {
            throw new \InvalidArgumentException('Request timeout must be positive.');
        }
    }

    public function process(Request $request, Response $response, RequestHandlerInterface $next): Response
    {
        $deadline = new Deadline($this->seconds);
        $this->container->scopedInstance(Deadline::class, $deadline);
        $result = $next->handle($request, $response);
        $deadline->throwIfExpired();
        return $result;
    }
}

