<?php

declare(strict_types=1);

namespace Nimbus\RateLimit;

use Pam\Http\Request;
use Pam\Http\Response;

/**
 * A fixed-window limit, as Laravel's `Illuminate\Cache\RateLimiting\Limit`:
 *
 * ```php
 * $limiter->for('buck', fn (Request $request) => Limit::perMinute(120)->by($accountId));
 * ```
 */
final readonly class Limit
{
    /** @var ?\Closure(Request, Response, array<string, string>): Response */
    public ?\Closure $responseCallback;

    /**
     * @param ?\Closure(Request, Response, array<string, string>): Response $responseCallback
     */
    public function __construct(
        public string $key = '',
        public int $maxAttempts = 60,
        public int $decaySeconds = 60,
        ?\Closure $responseCallback = null,
    ) {
        if ($maxAttempts < 1 || $decaySeconds < 1) {
            throw new \InvalidArgumentException('A rate limit needs at least one attempt per window of at least one second.');
        }
        $this->responseCallback = $responseCallback;
    }

    public static function perSecond(int $maxAttempts, int $decaySeconds = 1): self
    {
        return new self('', $maxAttempts, $decaySeconds);
    }

    public static function perMinute(int $maxAttempts, int $decayMinutes = 1): self
    {
        return new self('', $maxAttempts, 60 * $decayMinutes);
    }

    public static function perMinutes(int $decayMinutes, int $maxAttempts): self
    {
        return new self('', $maxAttempts, 60 * $decayMinutes);
    }

    public static function perHour(int $maxAttempts, int $decayHours = 1): self
    {
        return new self('', $maxAttempts, 3600 * $decayHours);
    }

    public static function perDay(int $maxAttempts, int $decayDays = 1): self
    {
        return new self('', $maxAttempts, 86_400 * $decayDays);
    }

    /** No limit: the request passes without a hit. */
    public static function none(): self
    {
        return new self('', PHP_INT_MAX, 1);
    }

    public function isUnlimited(): bool
    {
        return $this->maxAttempts === PHP_INT_MAX;
    }

    /** The key the attempts are counted under (an account id, an IP...). */
    public function by(string|int $key): self
    {
        return new self((string) $key, $this->maxAttempts, $this->decaySeconds, $this->responseCallback);
    }

    /**
     * The response sent instead of the 429 error; receives the rate-limit headers.
     *
     * @param \Closure(Request, Response, array<string, string>): Response $callback
     */
    public function response(\Closure $callback): self
    {
        return new self($this->key, $this->maxAttempts, $this->decaySeconds, $callback);
    }
}
