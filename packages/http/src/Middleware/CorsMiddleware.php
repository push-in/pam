<?php

declare(strict_types=1);

namespace Pam\Api\Middleware;

use Pam\Contracts\Http\MiddlewareInterface;
use Pam\Contracts\Http\RequestHandlerInterface;
use Pam\Http\Request;
use Pam\Http\Response;

final readonly class CorsMiddleware implements MiddlewareInterface
{
    /**
     * @param list<string> $origins
     * @param list<string> $methods
     * @param list<string> $headers
     */
    public function __construct(
        private array $origins,
        private array $methods = ['GET', 'POST', 'PUT', 'PATCH', 'DELETE', 'OPTIONS'],
        private array $headers = ['content-type', 'authorization'],
        private bool $credentials = false,
        private int $maxAge = 600,
    ) {
        if ($origins === [] || $maxAge < 0) {
            throw new \InvalidArgumentException('CORS requires origins and a non-negative max age.');
        }
    }

    public function process(Request $request, Response $response, RequestHandlerInterface $next): Response
    {
        $origin = $request->getHeader('origin');
        if ($origin === null || (!in_array('*', $this->origins, true) && !in_array($origin, $this->origins, true))) {
            return $next->handle($request, $response);
        }

        $allowedOrigin = in_array('*', $this->origins, true) && !$this->credentials ? '*' : $origin;
        $response
            ->header('access-control-allow-origin', $allowedOrigin)
            ->header('vary', 'origin');
        if ($this->credentials) {
            $response->header('access-control-allow-credentials', 'true');
        }
        if ($request->method === 'OPTIONS') {
            $requestedMethod = strtoupper($request->getHeader('access-control-request-method') ?? '');
            if ($requestedMethod === '' || !in_array($requestedMethod, $this->methods, true)) {
                return $response->json(['error' => 'CORS method is not allowed'], 403);
            }
            $requestedHeaders = array_values(array_filter(array_map(
                static fn (string $header): string => strtolower(trim($header)),
                explode(',', $request->getHeader('access-control-request-headers') ?? ''),
            )));
            foreach ($requestedHeaders as $requestedHeader) {
                if (!in_array($requestedHeader, array_map('strtolower', $this->headers), true)) {
                    return $response->json(['error' => 'CORS header is not allowed'], 403);
                }
            }
            return $response
                ->status(204)
                ->header('access-control-allow-methods', implode(', ', $this->methods))
                ->header('access-control-allow-headers', implode(', ', $this->headers))
                ->header('access-control-max-age', (string) $this->maxAge)
                ->send(null);
        }
        return $next->handle($request, $response);
    }
}
