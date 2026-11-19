<?php

declare(strict_types=1);

namespace Nimbus\Middleware;

use Nimbus\Container\Container;
use Nimbus\Http\CookieCodec;
use Nimbus\Http\CookieJar;
use Nimbus\Http\RequestCookies;
use Pam\Contracts\Http\MiddlewareInterface;
use Pam\Contracts\Http\RequestHandlerInterface;
use Pam\Http\Request;
use Pam\Http\Response;

/**
 * Decrypts incoming cookies into the scoped `RequestCookies` (values that fail to decrypt are
 * dropped), registers a scoped `CookieJar` and writes its queued cookies encrypted.
 * Cookies in `$except` pass through untouched; cookies in `$signed` are only signed.
 */
final readonly class EncryptCookiesMiddleware implements MiddlewareInterface
{
    /**
     * @param list<string> $except
     * @param list<string> $signed
     */
    public function __construct(
        private Container $container,
        private CookieCodec $codec,
        private array $except = ['nimbus_session', 'XSRF-TOKEN'],
        private array $signed = [],
    ) {
    }

    public function process(Request $request, Response $response, RequestHandlerInterface $next): Response
    {
        $decoded = [];
        foreach (RequestCookies::parse($request->getHeader('cookie') ?? '') as $name => $value) {
            $plain = match (true) {
                in_array($name, $this->except, true) => $value,
                in_array($name, $this->signed, true) => $this->codec->verify($name, $value),
                default => $this->codec->decrypt($name, $value),
            };
            if ($plain !== null) {
                $decoded[$name] = $plain;
            }
        }
        $jar = new CookieJar();
        $this->container
            ->scopedInstance(RequestCookies::class, new RequestCookies($decoded))
            ->scopedInstance(CookieJar::class, $jar);
        $response = $next->handle($request, $response);
        foreach ($jar->queued() as $cookie) {
            if ($cookie->maxAge >= 0 && !in_array($cookie->name, $this->except, true)) {
                $cookie = $cookie->withValue(in_array($cookie->name, $this->signed, true)
                    ? $this->codec->sign($cookie->name, $cookie->value)
                    : $this->codec->encrypt($cookie->name, $cookie->value));
            }
            $response->addHeader('set-cookie', $cookie->toHeader());
        }
        return $response;
    }
}
