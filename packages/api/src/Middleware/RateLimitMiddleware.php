<?php

declare(strict_types=1);

namespace Pam\Api\Middleware;

use Pam\Contracts\Http\MiddlewareInterface;
use Pam\Contracts\Http\RequestHandlerInterface;
use Pam\Http\Request;
use Pam\Http\Response;

final class RateLimitMiddleware implements MiddlewareInterface
{
    /** @var array<string, array{tokens: float, updatedAt: float}> */
    private array $buckets = [];

    public function __construct(
        private readonly int $requestsPerSecond,
        private readonly int $burst = 0,
        private readonly int $maxBuckets = 65_536,
        private readonly float $idleTtlSeconds = 300.0,
    ) {
        if ($requestsPerSecond < 1 || $burst < 0 || $maxBuckets < 1 || $idleTtlSeconds <= 0) {
            throw new \InvalidArgumentException('Rate limit configuration is invalid.');
        }
    }

    public function process(Request $request, Response $response, RequestHandlerInterface $next): Response
    {
        $key = is_string($_SERVER['REMOTE_ADDR'] ?? null) ? $_SERVER['REMOTE_ADDR'] : 'unknown';
        $now = microtime(true);
        if (!isset($this->buckets[$key]) && count($this->buckets) >= $this->maxBuckets) {
            $this->buckets = array_filter(
                $this->buckets,
                fn (array $entry): bool => $entry['updatedAt'] >= $now - $this->idleTtlSeconds,
            );
            if (count($this->buckets) >= $this->maxBuckets) {
                return $response
                    ->header('retry-after', '1')
                    ->json(['error' => 'Too Many Requests'], 429);
            }
        }
        $capacity = $this->burst > 0 ? $this->burst : $this->requestsPerSecond;
        $bucket = $this->buckets[$key] ?? ['tokens' => (float) $capacity, 'updatedAt' => $now];
        $elapsed = max(0.0, $now - $bucket['updatedAt']);
        $tokens = min((float) $capacity, $bucket['tokens'] + ($elapsed * $this->requestsPerSecond));
        if ($tokens < 1.0) {
            $this->buckets[$key] = ['tokens' => $tokens, 'updatedAt' => $now];
            return $response
                ->status(429)
                ->header('retry-after', '1')
                ->json(['error' => 'Too Many Requests'], 429);
        }
        $this->buckets[$key] = ['tokens' => $tokens - 1.0, 'updatedAt' => $now];
        return $next->handle($request, $response);
    }
}
