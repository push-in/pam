<?php

declare(strict_types=1);

namespace Pam\Api\Testing;

final readonly class TestResponse
{
    /** @param array{status: int, headers: array<string, list<string>>, body: string, chunks: list<string>} $response */
    public function __construct(private array $response)
    {
    }

    public function status(): int
    {
        return $this->response['status'];
    }

    public function body(): string
    {
        return $this->response['body'];
    }

    public function json(): mixed
    {
        return json_decode($this->body(), true, 512, JSON_THROW_ON_ERROR);
    }

    public function header(string $name): ?string
    {
        $values = $this->response['headers'][strtolower($name)] ?? null;
        return $values === null ? null : implode(', ', $values);
    }

    public function assertStatus(int $expected): self
    {
        if ($this->status() !== $expected) {
            throw new \RuntimeException("Expected status {$expected}; received {$this->status()}.");
        }
        return $this;
    }

    public function assertSuccessful(): self
    {
        $status = $this->status();
        if ($status < 200 || $status >= 300) {
            throw new \RuntimeException("Expected a successful response; received status {$status}.");
        }
        return $this;
    }

    public function assertHeader(string $name, ?string $expected = null): self
    {
        $actual = $this->header($name);
        if ($actual === null || ($expected !== null && $actual !== $expected)) {
            throw new \RuntimeException("Response header {$name} did not contain the expected value.");
        }
        return $this;
    }

    /** @param array<array-key, mixed> $expected */
    public function assertJson(array $expected): self
    {
        $actual = $this->json();
        if (!is_array($actual) || $actual !== $expected) {
            throw new \RuntimeException('Response JSON did not exactly match the expected payload.');
        }
        return $this;
    }

    public function assertJsonPath(string $path, mixed $expected): self
    {
        $value = $this->json();
        foreach (explode('.', $path) as $segment) {
            if (!is_array($value) || !array_key_exists($segment, $value)) {
                throw new \RuntimeException("JSON path {$path} does not exist.");
            }
            $value = $value[$segment];
        }
        if ($value !== $expected) {
            throw new \RuntimeException("JSON path {$path} does not contain the expected value.");
        }
        return $this;
    }
}
