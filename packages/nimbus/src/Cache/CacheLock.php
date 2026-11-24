<?php

declare(strict_types=1);

namespace Nimbus\Cache;

/**
 * A lock kept in any cache store through its atomic `add()`. The memory store is
 * per worker; use the Redis store to lock across workers and servers.
 */
final class CacheLock extends Lock
{
    public function __construct(private readonly Store $store, string $name, int $seconds = 0, ?string $owner = null)
    {
        parent::__construct($name, $seconds, $owner);
    }

    public function acquire(): bool
    {
        return $this->store->add($this->key(), $this->owner, $this->seconds > 0 ? $this->seconds : null);
    }

    public function release(): bool
    {
        if (!$this->isOwnedByCurrentProcess()) {
            return false;
        }
        $this->store->forget($this->key());
        return true;
    }

    public function forceRelease(): void
    {
        $this->store->forget($this->key());
    }

    protected function currentOwner(): ?string
    {
        $owner = $this->store->get($this->key());
        return is_string($owner) ? $owner : null;
    }

    private function key(): string
    {
        return 'lock:' . $this->name;
    }
}
