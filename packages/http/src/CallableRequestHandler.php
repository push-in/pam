<?php

declare(strict_types=1);

namespace Pam\Api;

use Pam\Api\Http\Responsable;
use Pam\Contracts\Http\RequestHandlerInterface;
use Pam\Http\Request;
use Pam\Http\Response;

final readonly class CallableRequestHandler implements RequestHandlerInterface
{
    private \Closure $handler;

    public function __construct(callable $handler)
    {
        $this->handler = \Closure::fromCallable($handler);
    }

    public function handle(Request $request, Response $response): Response
    {
        $result = ($this->handler)($request, $response);
        if ($result instanceof Response) {
            return $result;
        }
        if ($result instanceof Responsable) {
            return $result->toResponse($request, $response);
        }
        if ($result !== null && $response->isEmpty()) {
            $response->send($result);
        }
        return $response;
    }
}
