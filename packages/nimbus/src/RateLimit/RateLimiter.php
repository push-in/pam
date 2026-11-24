<?php

declare(strict_types=1);

namespace Nimbus\RateLimit;

use Nimbus\Cache\Repository;
use Pam\Http\Request;

/**
 * Named limiters and fixed-window attempt counters kept in a cache repository (the
 * memory store in one worker, the Redis store across servers), as Laravel's `RateLimiter`.
 *
 * ```php
 * $limiter = $app->container()->get(RateLimiter::class);
 * $limiter->for('buck-ask', fn (Request $request) => Limit::perMinute(10)->by($request->header('x-account') ?? ''));
 * $app->post('/ask', AskController::class)->rateLimit('buck-ask');
 * $app->post('/login', LoginController::class)->throttle(3, 1); // throttle:3,1
 * ```
 */
final class RateLimiter
{
    /** @var array<string, \Closure(Request): (Limit|list<Limit>)> */
    private array $limiters = [];

    public function __construct(private readonly Repository $cache, private readonly string $prefix = 'rate-limit:')
    {
    }

    /**
     * Registers a named limiter; the callback returns one `Limit` or several.
     *
     * @param \Closure(Request): (Limit|list<Limit>) $callback
     */
    public function for(string $name, \Closure $callback): self
    {
        if ($name === '') {
            throw new \InvalidArgumentException('A rate limiter needs a name.');
        }
        $this->limiters[$name] = $callback;
        return $this;
    }

    /** @return ?\Closure(Request): (Limit|list<Limit>) */
    public function limiter(string $name): ?\Closure
    {
        return $this->limiters[$name] ?? null;
    }

    /**
     * Runs the callback when the key has attempts left, counting the attempt; false otherwise.
     *
     * @template T
     * @param \Closure(): T $callback
     * @return T|bool The callback's result (true when it returns null), false when limited.
     */
    public function attempt(string $key, int $maxAttempts, \Closure $callback, int $decaySeconds = 60): mixed
    {
        if ($this->tooManyAttempts($key, $maxAttempts)) {
            return false;
        }
        $this->hit($key, $decaySeconds);
        $result = $callback();
        return $result ?? true;
    }

    public function tooManyAttempts(string $key, int $maxAttempts): bool
    {
        if ($this->attempts($key) >= $maxAttempts) {
            if ($this->cache->has($this->timerKey($key))) {
                return true;
            }
            $this->resetAttempts($key);
        }
        return false;
    }

    /** Counts one attempt in the key's window and returns the attempts of the window. */
    public function hit(string $key, int $decaySeconds = 60): int
    {
        return $this->increment($key, $decaySeconds);
    }

    public function increment(string $key, int $decaySeconds = 60, int $amount = 1): int
    {
        if ($decaySeconds < 1) {
            throw new \InvalidArgumentException('The rate-limit window must be at least one second.');
        }
        $this->cache->add($this->timerKey($key), time() + $decaySeconds, $decaySeconds);
        $this->cache->add($this->counterKey($key), 0, $decaySeconds);
        $hits = $this->cache->increment($this->counterKey($key), $amount);
        if ($hits === $amount) {
            // A first hit after the counter expired between both calls starts a new window.
            $this->cache->put($this->counterKey($key), $hits, $decaySeconds);
        }
        return $hits;
    }

    public function attempts(string $key): int
    {
        $attempts = $this->cache->get($this->counterKey($key), 0);
        return is_numeric($attempts) ? (int) $attempts : 0;
    }

    public function remaining(string $key, int $maxAttempts): int
    {
        return max(0, $maxAttempts - $this->attempts($key));
    }

    /** Seconds until the key's window resets. */
    public function availableIn(string $key): int
    {
        $resetsAt = $this->cache->get($this->timerKey($key));
        return is_numeric($resetsAt) ? max(0, (int) $resetsAt - time()) : 0;
    }

    /** Unix time when the key's window resets (`X-RateLimit-Reset`). */
    public function resetsAt(string $key): int
    {
        return time() + $this->availableIn($key);
    }

    public function resetAttempts(string $key): void
    {
        $this->cache->forget($this->counterKey($key));
    }

    public function clear(string $key): void
    {
        $this->resetAttempts($key);
        $this->cache->forget($this->timerKey($key));
    }

    private function counterKey(string $key): string
    {
        return $this->prefix . $key;
    }

    private function timerKey(string $key): string
    {
        return $this->prefix . $key . ':timer';
    }
}
