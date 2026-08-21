<?php

declare(strict_types=1);

namespace Pam\Api\Middleware;

use Pam\Api\Database\EloquentManager;
use Pam\Contracts\Http\MiddlewareInterface;
use Pam\Contracts\Http\RequestHandlerInterface;
use Pam\Http\Request;
use Pam\Http\Response;

final readonly class EloquentLifecycleMiddleware implements MiddlewareInterface
{
    public function __construct(private EloquentManager $eloquent)
    {
    }

    public function process(Request $request, Response $response, RequestHandlerInterface $next): Response
    {
        try {
            return $next->handle($request, $response);
        } finally {
            $this->eloquent->releaseCurrentRequest();
        }
    }
}
