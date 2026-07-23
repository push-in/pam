<?php

declare(strict_types=1);

namespace Pam\Api;

use Pam\Contracts\Http\MiddlewareInterface;
use Pam\Contracts\Http\RequestHandlerInterface;
use Pam\Http\Request;
use Pam\Http\Response;

final class Pipeline implements RequestHandlerInterface
{
    private RequestHandlerInterface $compiled;

    /** @param list<MiddlewareInterface|callable> $middleware */
    public function __construct(array $middleware, RequestHandlerInterface $destination)
    {
        $next = $destination;
        foreach (array_reverse($middleware) as $layer) {
            $downstream = $next;
            $next = new CallableRequestHandler(
                static function (Request $request, Response $response) use ($layer, $downstream): Response {
                    $result = $layer instanceof MiddlewareInterface
                        ? $layer->process($request, $response, $downstream)
                        : $layer($request, $response, $downstream);
                    if (!$result instanceof Response) {
                        throw new \UnexpectedValueException('Pam middleware must return Pam\\Http\\Response.');
                    }
                    return $result;
                },
            );
        }
        $this->compiled = $next;
    }

    public function handle(Request $request, Response $response): Response
    {
        return $this->compiled->handle($request, $response);
    }
}
