<?php

declare(strict_types=1);

namespace Nimbus\Middleware;

use Nimbus\Http\FormData;
use Pam\Contracts\Http\MiddlewareInterface;
use Pam\Contracts\Http\RequestHandlerInterface;
use Pam\Http\Request;
use Pam\Http\Response;

/**
 * Lets HTML forms send PUT, PATCH and DELETE: a POST with a `_method` field (`@method` in
 * views) or an `X-HTTP-Method-Override` header is routed with that method. Register it as
 * global middleware so it runs before routing.
 */
final readonly class MethodOverrideMiddleware implements MiddlewareInterface
{
    private const array METHODS = ['PUT', 'PATCH', 'DELETE'];

    public function process(Request $request, Response $response, RequestHandlerInterface $next): Response
    {
        if ($request->method !== 'POST') {
            return $next->handle($request, $response);
        }
        $method = strtoupper($request->getHeader('x-http-method-override') ?? FormData::field($request, '_method') ?? '');
        if (!in_array($method, self::METHODS, true)) {
            return $next->handle($request, $response);
        }
        return $next->handle(new Request(
            $method,
            $request->path,
            $request->query(),
            $request->headers(),
            $request->body(),
            $request->routeParameters(),
        ), $response);
    }
}
