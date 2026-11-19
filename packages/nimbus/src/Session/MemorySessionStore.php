<?php

declare(strict_types=1);

namespace Nimbus\Session;

/** Sessions in the worker's memory: tests and single-worker development only. */
final class MemorySessionStore implements SessionStore
{
    /** @var array<string, array{payload: string, expiresAt: int}> */
    private array $sessions = [];

    /** @var \Closure(): int */
    private readonly \Closure $clock;

    /** @param (\Closure(): int)|null $clock */
    public function __construct(?\Closure $clock = null)
    {
        $this->clock = $clock ?? time(...);
    }

    public function read(string $id): ?array
    {
        $session = $this->sessions[$id] ?? null;
        if ($session === null || $session['expiresAt'] <= ($this->clock)()) {
            return null;
        }
        return SessionPayload::decode($session['payload']);
    }

    public function write(string $id, array $data, int $lifetimeSeconds): void
    {
        $this->sessions[$id] = ['payload' => SessionPayload::encode($data), 'expiresAt' => ($this->clock)() + $lifetimeSeconds];
    }

    public function destroy(string $id): void
    {
        unset($this->sessions[$id]);
    }

    public function gc(): int
    {
        $now = ($this->clock)();
        $removed = 0;
        foreach ($this->sessions as $id => $session) {
            if ($session['expiresAt'] <= $now) {
                unset($this->sessions[$id]);
                ++$removed;
            }
        }
        return $removed;
    }
}
