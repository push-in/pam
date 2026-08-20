<?php

declare(strict_types=1);

namespace Pam\Api\Middleware;

use Pam\Contracts\Http\MiddlewareInterface;
use Pam\Contracts\Http\RequestHandlerInterface;
use Pam\Http\Request;
use Pam\Http\Response;
use Pam\Api\RateLimit\MemoryRateLimitStore;
use Pam\Api\RateLimit\RateLimitStore;

final class RateLimitMiddleware implements MiddlewareInterface
{
    private readonly RateLimitStore $store;

    /** @var \Closure(Request): string */
    private readonly \Closure $keyResolver;

    public function __construct(
        private readonly int $requestsPerSecond,
        private readonly int $burst = 0,
        private readonly int $maxBuckets = 65_536,
        private readonly float $idleTtlSeconds = 300.0,
        ?RateLimitStore $store = null,
        ?callable $keyResolver = null,
    ) {
        if ($requestsPerSecond < 1 || $burst < 0 || $maxBuckets < 1 || $idleTtlSeconds <= 0) {
            throw new \InvalidArgumentException('Rate limit configuration is invalid.');
        }
        $this->store = $store ?? new MemoryRateLimitStore($maxBuckets, $idleTtlSeconds);
        $this->keyResolver = $keyResolver === null
            ? static fn (Request $request): string => is_string($_SERVER['REMOTE_ADDR'] ?? null)
                ? $_SERVER['REMOTE_ADDR']
                : 'unknown'
            : \Closure::fromCallable($keyResolver);
    }

    public function process(Request $request, Response $response, RequestHandlerInterface $next): Response
    {
        $key = ($this->keyResolver)($request);
        if ($key === '') {
            throw new \UnexpectedValueException('The rate-limit key resolver returned an empty key.');
        }
        $capacity = $this->burst > 0 ? $this->burst : $this->requestsPerSecond;
        $decision = $this->store->consume($key, $this->requestsPerSecond, $capacity, microtime(true));
        $response
            ->header('x-ratelimit-limit', (string) $decision->limit)
            ->header('x-ratelimit-remaining', (string) $decision->remaining);
        if (!$decision->allowed) {
            return $response
                ->header('retry-after', (string) $decision->retryAfterSeconds)
                ->json([
                    'type' => 'https://pam.dev/problems/6',
                    'title' => 'Too Many Requests',
                    'status' => 429,
                    'code' => 6,
                ], 429);
        }
        return $next->handle($request, $response);
    }
}
