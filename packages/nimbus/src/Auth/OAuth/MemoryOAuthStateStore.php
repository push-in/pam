<?php

declare(strict_types=1);

namespace Nimbus\Auth\OAuth;

/** Bounded per-worker state store for tests and single-worker development. */
final class MemoryOAuthStateStore implements OAuthStateStore
{
    /** @var array<string, string> */
    private array $states = [];

    public function __construct(private readonly int $maximumEntries = 10_000)
    {
        if ($maximumEntries < 1) {
            throw new \InvalidArgumentException('The state store needs room for at least one entry.');
        }
    }

    public function put(string $state, array $data, int $expiresAt): void
    {
        if (count($this->states) >= $this->maximumEntries) {
            array_shift($this->states);
        }
        $this->states[OAuthStateCodec::key($state)] = OAuthStateCodec::encode($data, $expiresAt);
    }

    public function pull(string $state, int $now): ?array
    {
        $key = OAuthStateCodec::key($state);
        $payload = $this->states[$key] ?? null;
        unset($this->states[$key]);
        return OAuthStateCodec::decode($payload, $now);
    }
}
