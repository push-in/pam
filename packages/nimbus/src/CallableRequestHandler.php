<?php

declare(strict_types=1);

namespace Nimbus;

use Nimbus\Http\Responsable;
use Pam\Contracts\Http\RequestHandlerInterface;
use Pam\Http\Request;
use Pam\Http\Response;

/**
 * Wraps a callable as the request handler of the pipeline.
 *
 * @internal Implementation detail of Nimbus, excluded from the semver promise; use the public API that wraps it.
 */
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
        if ((is_array($result) || $result instanceof \JsonSerializable) && $response->isEmpty()) {
            // The status the action or a middleware set is kept.
            return $response
                ->header('content-type', 'application/json; charset=utf-8')
                ->send(\Nimbus\Http\JsonEncoding::encode($result));
        }
        if ($result !== null && $response->isEmpty()) {
            $response->send($result);
        }
        return $response;
    }
}
