<?php

declare(strict_types=1);

namespace Nimbus\Notifications\Http;

final readonly class HttpResponse
{
    /** @var array<string, string> */
    public array $headers;

    /** @param array<string, string> $headers Names are lowercased. */
    public function __construct(
        public int $status,
        array $headers = [],
        public string $body = '',
    ) {
        $this->headers = array_change_key_case($headers, CASE_LOWER);
    }

    public function header(string $name): ?string
    {
        return $this->headers[strtolower($name)] ?? null;
    }

    public function successful(): bool
    {
        return $this->status >= 200 && $this->status < 300;
    }

    /** @return array<mixed> Decoded JSON object, or an empty array. */
    public function json(): array
    {
        try {
            $decoded = json_decode($this->body, true, 64, JSON_THROW_ON_ERROR);
        } catch (\JsonException) {
            return [];
        }
        return is_array($decoded) ? $decoded : [];
    }
}
