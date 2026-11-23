<?php

declare(strict_types=1);

namespace Nimbus\Http\Client;

/**
 * A request as the transport sends it; HTTP fakes pass it to `assertSent()` callbacks.
 */
final readonly class OutgoingRequest
{
    /** @var array<string, string> Header names are lowercased. */
    public array $headers;

    /** @param array<string, string> $headers */
    public function __construct(
        public string $method,
        public string $url,
        array $headers = [],
        public string $body = '',
        public float $timeout = 30.0,
        public bool $verifyTls = true,
        public int $maxRedirects = 5,
    ) {
        if ($timeout <= 0 || $timeout > 3600) {
            throw new \InvalidArgumentException('HTTP timeout must be between 0 and 3600 seconds.');
        }
        if (!in_array(parse_url($url, PHP_URL_SCHEME), ['http', 'https'], true)) {
            throw new \InvalidArgumentException("HTTP URL must use http or https: {$url}.");
        }
        if ($maxRedirects < 0) {
            throw new \InvalidArgumentException('Maximum redirects cannot be negative.');
        }
        $this->headers = array_change_key_case($headers, CASE_LOWER);
    }

    public function method(): string
    {
        return $this->method;
    }

    public function url(): string
    {
        return $this->url;
    }

    public function body(): string
    {
        return $this->body;
    }

    public function header(string $name): ?string
    {
        return $this->headers[strtolower($name)] ?? null;
    }

    public function hasHeader(string $name, ?string $value = null): bool
    {
        $actual = $this->header($name);
        return $actual !== null && ($value === null || $actual === $value);
    }

    /**
     * The JSON or form body as an array, else the query string of a body-less request.
     *
     * @return array<array-key, mixed>
     */
    public function data(): array
    {
        if ($this->body === '') {
            parse_str((string) parse_url($this->url, PHP_URL_QUERY), $query);
            return $query;
        }
        if (str_starts_with($this->header('content-type') ?? '', 'application/x-www-form-urlencoded')) {
            parse_str($this->body, $form);
            return $form;
        }
        try {
            $decoded = json_decode($this->body, true, 512, JSON_THROW_ON_ERROR);
        } catch (\JsonException) {
            return [];
        }
        return is_array($decoded) ? $decoded : [];
    }
}
