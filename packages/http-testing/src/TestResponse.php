<?php

declare(strict_types=1);

namespace Pam\Http\Testing;

final readonly class TestResponse
{
    /**
     * @param array<string, list<string>> $headers
     * @param list<string> $chunks
     */
    public function __construct(
        public int $status,
        public array $headers,
        public string $body,
        public array $chunks = [],
    ) {
    }

    public function header(string $name, ?string $default = null): ?string
    {
        $values = $this->headers[strtolower($name)] ?? [];
        return $values === [] ? $default : implode(', ', $values);
    }

    public function json(): mixed
    {
        return json_decode($this->body, true, 512, JSON_THROW_ON_ERROR);
    }

    public function assertStatus(int $expected): self
    {
        if ($this->status !== $expected) {
            throw new \RuntimeException("Expected HTTP {$expected}, received {$this->status}.");
        }
        return $this;
    }

    public function assertSuccessful(): self
    {
        if ($this->status < 200 || $this->status >= 300) {
            throw new \RuntimeException("Expected a successful response, received HTTP {$this->status}.");
        }
        return $this;
    }

    public function assertHeader(string $name, ?string $expected = null): self
    {
        $actual = $this->header($name);
        if ($actual === null || ($expected !== null && $actual !== $expected)) {
            throw new \RuntimeException("Unexpected {$name} response header.");
        }
        return $this;
    }

    /** @param array<array-key, mixed> $expected */
    public function assertJson(array $expected): self
    {
        $actual = $this->json();
        if (!is_array($actual) || $actual !== $expected) {
            throw new \RuntimeException('The response JSON did not match the expected payload.');
        }
        return $this;
    }

    public function assertJsonPath(string $path, mixed $expected): self
    {
        $value = $this->json();
        foreach (explode('.', $path) as $segment) {
            if (!is_array($value) || !array_key_exists($segment, $value)) {
                throw new \RuntimeException("The response JSON path {$path} does not exist.");
            }
            $value = $value[$segment];
        }
        if ($value !== $expected) {
            throw new \RuntimeException("The response JSON path {$path} did not match the expected value.");
        }
        return $this;
    }
}
