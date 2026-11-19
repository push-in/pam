<?php

declare(strict_types=1);

namespace Nimbus\Notifications\Http;

/**
 * An outbound request of a notification channel.
 */
final readonly class HttpRequest
{
    /** @param array<string, string> $headers */
    public function __construct(
        public string $method,
        public string $url,
        public array $headers = [],
        public string $body = '',
        public float $timeout = 10.0,
        public bool $http2 = false,
    ) {
        if ($timeout <= 0 || $timeout > 300) {
            throw new \InvalidArgumentException('HTTP timeout must be between 0 and 300 seconds.');
        }
        $scheme = parse_url($url, PHP_URL_SCHEME);
        if (!in_array($scheme, ['http', 'https'], true)) {
            throw new \InvalidArgumentException('HTTP URL must use http or https.');
        }
    }
}
