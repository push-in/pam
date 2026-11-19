<?php

declare(strict_types=1);

namespace Nimbus\Http;

/**
 * Cookies queued during one request. `EncryptCookiesMiddleware` registers a jar in the
 * request scope (inject it into controllers) and writes the queued cookies, encrypted,
 * on the response.
 */
final class CookieJar
{
    /** @var array<string, Cookie> */
    private array $queued = [];

    public function queue(Cookie $cookie): self
    {
        $this->queued[$cookie->name . ';' . $cookie->path . ';' . ($cookie->domain ?? '')] = $cookie;
        return $this;
    }

    /** Queues a cookie with the default attributes (secure, HttpOnly, SameSite=Lax). */
    public function make(string $name, string $value, int $maxAge = 0): self
    {
        return $this->queue(new Cookie($name, $value, $maxAge));
    }

    public function forget(string $name, string $path = '/', ?string $domain = null): self
    {
        return $this->queue(Cookie::forget($name, $path, $domain));
    }

    /** @return list<Cookie> */
    public function queued(): array
    {
        return array_values($this->queued);
    }
}
