<?php

declare(strict_types=1);

namespace Nimbus\Middleware;

use Nimbus\Http\ClientIpResolver;
use Pam\Contracts\Http\MiddlewareInterface;
use Pam\Contracts\Http\RequestHandlerInterface;
use Pam\Http\Request;
use Pam\Http\Response;

/**
 * Security headers on every response, error responses included: the headers are set before the
 * handler runs, so the problem response the application builds from an exception carries them.
 *
 * HSTS goes out on HTTPS requests. Behind a proxy that terminates TLS, `$_SERVER['HTTPS']` is
 * off; pass the proxies in `$trustedProxies` (addresses or CIDR ranges) and their
 * `X-Forwarded-Proto: https` counts; a forged header from anyone else is ignored. Under PAM with
 * the runtime's `trustedProxies`, `REMOTE_ADDR` is already the client, so set
 * `$trustForwardedProto` instead: browsers ignore HSTS received over plain HTTP, so a forged
 * header gains nothing.
 *
 * `$contentSecurityPolicy` (off by default; `DEFAULT_CSP` is a strict starting point for HTML
 * applications) is only set when the handler did not set its own, as the admin panel does.
 */
final readonly class SecurityHeadersMiddleware implements MiddlewareInterface
{
    public const string DEFAULT_CSP = "default-src 'self'; base-uri 'self'; form-action 'self'; frame-ancestors 'none'; object-src 'none'";

    private ?ClientIpResolver $proxies;

    /** @param list<string> $trustedProxies */
    public function __construct(
        private bool $hsts = true,
        array $trustedProxies = [],
        private ?string $contentSecurityPolicy = null,
        private bool $trustForwardedProto = false,
    ) {
        $this->proxies = $trustedProxies === [] ? null : new ClientIpResolver($trustedProxies);
        if ($contentSecurityPolicy !== null && ($contentSecurityPolicy === '' || preg_match('/[\r\n]/', $contentSecurityPolicy) === 1)) {
            throw new \InvalidArgumentException('The Content-Security-Policy must be a non-empty single line.');
        }
    }

    public function process(Request $request, Response $response, RequestHandlerInterface $next): Response
    {
        $this->apply($request, $response);
        $result = $next->handle($request, $response);
        if ($result !== $response) {
            $this->apply($request, $result);
        }
        if ($this->contentSecurityPolicy !== null && !isset($result->export()['headers']['content-security-policy'])) {
            $result->header('content-security-policy', $this->contentSecurityPolicy);
        }
        return $result;
    }

    private function apply(Request $request, Response $response): void
    {
        $response
            ->header('x-content-type-options', 'nosniff')
            ->header('x-frame-options', 'DENY')
            ->header('referrer-policy', 'no-referrer')
            ->header('permissions-policy', 'camera=(), microphone=(), geolocation=()');
        if ($this->hsts && $this->secure($request)) {
            $response->header('strict-transport-security', 'max-age=31536000; includeSubDomains');
        }
    }

    private function secure(Request $request): bool
    {
        if (($_SERVER['HTTPS'] ?? 'off') === 'on') {
            return true;
        }
        $proto = $request->getHeader('x-forwarded-proto');
        if ($proto === null) {
            return false;
        }
        $peer = is_string($_SERVER['REMOTE_ADDR'] ?? null) ? $_SERVER['REMOTE_ADDR'] : '';
        if (!$this->trustForwardedProto && ($this->proxies === null || !$this->proxies->trusts($peer))) {
            return false;
        }
        // The proxy nearest to the application writes the last value.
        return strtolower(trim((string) strrchr(',' . $proto, ','), ", \t")) === 'https';
    }
}
