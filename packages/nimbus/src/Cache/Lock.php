<?php

declare(strict_types=1);

namespace Nimbus\Cache;

/**
 * An atomic lock with an owner token, as Laravel's `Cache::lock()`:
 *
 * ```php
 * Cache::lock('ask:' . $accountId, 30)->get(fn () => $this->ask($accountId));
 * Cache::lock('heartbeat', 10)->block(5, fn () => $this->beat()); // waits up to 5 s
 *
 * $lock = Cache::lock('report', 120);
 * if ($lock->get()) {
 *     try { ... } finally { $lock->release(); }
 * }
 * ```
 *
 * Only the owner releases the lock; `restoreLock($name, $owner)` rebuilds it in another
 * process (a queued job). `$seconds = 0` keeps the lock until it is released.
 */
abstract class Lock
{
    protected readonly string $owner;

    private int $sleepMilliseconds = 250;

    public function __construct(
        protected readonly string $name,
        protected readonly int $seconds = 0,
        ?string $owner = null,
    ) {
        if ($name === '' || $seconds < 0) {
            throw new \InvalidArgumentException('A cache lock needs a name and a non-negative lifetime.');
        }
        $this->owner = $owner ?? bin2hex(random_bytes(16));
    }

    /** Takes the lock unless another owner holds it. */
    abstract public function acquire(): bool;

    /** Frees the lock when this owner holds it; true when it was released. */
    abstract public function release(): bool;

    /** Frees the lock whoever holds it. */
    abstract public function forceRelease(): void;

    /** The owner token currently stored for the lock, null when it is free. */
    abstract protected function currentOwner(): ?string;

    /**
     * Without a callback, takes the lock and returns whether it was taken. With one, runs it
     * under the lock and releases it afterwards; false when the lock is held elsewhere.
     *
     * @template T
     * @param ?\Closure(): T $callback
     * @return ($callback is null ? bool : T|false)
     */
    public function get(?\Closure $callback = null): mixed
    {
        $acquired = $this->acquire();
        if (!$acquired || $callback === null) {
            return $acquired;
        }
        try {
            return $callback();
        } finally {
            $this->release();
        }
    }

    /**
     * Waits up to `$seconds` for the lock, then runs the callback as `get()` does.
     *
     * @template T
     * @param ?\Closure(): T $callback
     * @return ($callback is null ? true : T)
     * @throws LockTimeoutException
     */
    public function block(int $seconds, ?\Closure $callback = null): mixed
    {
        $deadline = microtime(true) + max(0, $seconds);
        while (!$this->acquire()) {
            if (microtime(true) + $this->sleepMilliseconds / 1000 > $deadline) {
                throw new LockTimeoutException("Could not acquire the cache lock [{$this->name}] in {$seconds} s.");
            }
            self::sleep($this->sleepMilliseconds);
        }
        if ($callback === null) {
            return true;
        }
        try {
            return $callback();
        } finally {
            $this->release();
        }
    }

    /** Milliseconds between the attempts of `block()` (250 by default). */
    public function betweenBlockedAttemptsSleepFor(int $milliseconds): static
    {
        if ($milliseconds < 1) {
            throw new \InvalidArgumentException('The lock retry interval must be positive.');
        }
        $this->sleepMilliseconds = $milliseconds;
        return $this;
    }

    public function name(): string
    {
        return $this->name;
    }

    public function owner(): string
    {
        return $this->owner;
    }

    public function isOwnedByCurrentProcess(): bool
    {
        return $this->currentOwner() === $this->owner;
    }

    public function isOwnedBy(?string $owner): bool
    {
        return $owner !== null && $this->currentOwner() === $owner;
    }

    /** Waits without blocking other requests of the worker when running in a PAM Fiber. */
    private static function sleep(int $milliseconds): void
    {
        if (\Fiber::getCurrent() !== null && function_exists('Pam\\Async\\delay')) {
            \Pam\Async\delay($milliseconds / 1000);
            return;
        }
        usleep($milliseconds * 1000);
    }
}
