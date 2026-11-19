<?php

declare(strict_types=1);

namespace Nimbus\Http;

use Pam\Http\Request;
use Pam\Http\Response;

/** A redirect response. Flash data goes through the injected `Session\Session`. */
final readonly class Redirect implements Responsable
{
    public function __construct(
        public string $location,
        public int $status = 302,
    ) {
        if ($status < 300 || $status > 399) {
            throw new \InvalidArgumentException('A redirect status must be 3xx.');
        }
        if ($location === '' || preg_match('/[\r\n]/', $location) === 1) {
            throw new \InvalidArgumentException('The redirect location is invalid.');
        }
    }

    public static function to(string $location, int $status = 302): self
    {
        return new self($location, $status);
    }

    /**
     * Back to the same-origin `Referer`, or to `$fallback`. Cross-origin referers are ignored
     * so the redirect cannot be used to leave the application.
     */
    public static function back(Request $request, string $fallback = '/', int $status = 302): self
    {
        $referer = $request->getHeader('referer');
        $host = $request->getHeader('host');
        if ($referer !== null && $host !== null) {
            $parts = parse_url($referer);
            if (is_array($parts) && ($parts['host'] ?? null) !== null
                && strtolower(($parts['host']) . (isset($parts['port']) ? ':' . $parts['port'] : '')) === strtolower($host)) {
                return new self(($parts['path'] ?? '/') . (isset($parts['query']) ? '?' . $parts['query'] : ''), $status);
            }
        }
        return new self($fallback, $status);
    }

    public function toResponse(Request $request, Response $response): Response
    {
        return $response->status($this->status)->header('location', $this->location)->send(null);
    }
}
