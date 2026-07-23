<?php

declare(strict_types=1);

namespace Pam\Api\Middleware;

use Pam\Contracts\Http\MiddlewareInterface;
use Pam\Contracts\Http\RequestHandlerInterface;
use Pam\Http\Request;
use Pam\Http\Response;

final readonly class SecurityHeadersMiddleware implements MiddlewareInterface
{
    public function __construct(private bool $hsts = true)
    {
    }

    public function process(Request $request, Response $response, RequestHandlerInterface $next): Response
    {
        $result = $next->handle($request, $response);
        $result
            ->header('x-content-type-options', 'nosniff')
            ->header('x-frame-options', 'DENY')
            ->header('referrer-policy', 'no-referrer')
            ->header('permissions-policy', 'camera=(), microphone=(), geolocation=()');
        if ($this->hsts && ($_SERVER['HTTPS'] ?? 'off') === 'on') {
            $result->header('strict-transport-security', 'max-age=31536000; includeSubDomains');
        }
        return $result;
    }
}
