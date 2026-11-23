<?php

declare(strict_types=1);

namespace Nimbus\Http\Client;

/**
 * A received HTTP response. 4xx and 5xx statuses are returned, not thrown; call `throw()`.
 */
final class Response
{
    /** @var array<string, list<string>> */
    private readonly array $headers;

    private mixed $decoded = null;

    private bool $decodedOnce = false;

    /** @param array<string, string|list<string>> $headers */
    public function __construct(
        private readonly int $status,
        private readonly string $body = '',
        array $headers = [],
        public readonly ?OutgoingRequest $request = null,
    ) {
        $normalized = [];
        foreach ($headers as $name => $values) {
            $normalized[strtolower($name)] = is_array($values) ? $values : [$values];
        }
        $this->headers = $normalized;
    }

    public function status(): int
    {
        return $this->status;
    }

    public function body(): string
    {
        return $this->body;
    }

    /**
     * The decoded JSON body, or the value at a dot-separated key such as `data.id`.
     */
    public function json(?string $key = null, mixed $default = null): mixed
    {
        if (!$this->decodedOnce) {
            $this->decodedOnce = true;
            try {
                $this->decoded = $this->body === '' ? null : json_decode($this->body, true, 512, JSON_THROW_ON_ERROR);
            } catch (\JsonException) {
                $this->decoded = null;
            }
        }
        if ($key === null) {
            return $this->decoded ?? $default;
        }
        $value = $this->decoded;
        foreach (explode('.', $key) as $segment) {
            if (!is_array($value) || !array_key_exists($segment, $value)) {
                return $default;
            }
            $value = $value[$segment];
        }
        return $value;
    }

    /** @return array<array-key, mixed> The decoded JSON object or list; empty for anything else. */
    public function array(): array
    {
        $decoded = $this->json();
        return is_array($decoded) ? $decoded : [];
    }

    public function header(string $name): ?string
    {
        $values = $this->headers[strtolower($name)] ?? [];
        return $values === [] ? null : implode(', ', $values);
    }

    /** @return array<string, list<string>> */
    public function headers(): array
    {
        return $this->headers;
    }

    public function successful(): bool
    {
        return $this->status >= 200 && $this->status < 300;
    }

    public function ok(): bool
    {
        return $this->status === 200;
    }

    public function redirect(): bool
    {
        return $this->status >= 300 && $this->status < 400;
    }

    public function failed(): bool
    {
        return $this->clientError() || $this->serverError();
    }

    public function clientError(): bool
    {
        return $this->status >= 400 && $this->status < 500;
    }

    public function serverError(): bool
    {
        return $this->status >= 500;
    }

    /**
     * Throws a RequestException for a 4xx or 5xx response; otherwise returns the response.
     *
     * @throws RequestException
     */
    public function throw(): self
    {
        if ($this->failed()) {
            throw new RequestException($this);
        }
        return $this;
    }

    public function withRequest(OutgoingRequest $request): self
    {
        return new self($this->status, $this->body, $this->headers, $request);
    }
}
