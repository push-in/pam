<?php

declare(strict_types=1);

namespace Nimbus\Session;

/** Validation messages by field, flashed to the next request (`$errors` in views). */
final readonly class ErrorBag implements \Countable
{
    /** @var array<string, list<string>> */
    private array $messages;

    /** @param array<array-key, mixed> $messages */
    public function __construct(array $messages = [])
    {
        $normalized = [];
        foreach ($messages as $field => $list) {
            foreach (is_array($list) ? $list : [$list] as $message) {
                if (is_scalar($message)) {
                    $normalized[(string) $field][] = (string) $message;
                }
            }
        }
        $this->messages = $normalized;
    }

    public function has(string $field): bool
    {
        return isset($this->messages[$field]);
    }

    public function first(?string $field = null, ?string $default = null): ?string
    {
        if ($field === null) {
            return $this->all()[0] ?? $default;
        }
        return $this->messages[$field][0] ?? $default;
    }

    /** @return list<string> */
    public function get(string $field): array
    {
        return $this->messages[$field] ?? [];
    }

    /** @return list<string> */
    public function all(): array
    {
        return array_merge(...array_values($this->messages));
    }

    public function any(): bool
    {
        return $this->messages !== [];
    }

    public function isEmpty(): bool
    {
        return $this->messages === [];
    }

    public function count(): int
    {
        return count($this->all());
    }

    /** @return array<string, list<string>> */
    public function toArray(): array
    {
        return $this->messages;
    }
}
