<?php

declare(strict_types=1);

namespace Pam\Api\Middleware;

use Pam\Api\Cache\CacheRecord;
use Pam\Api\Cache\ResponseCacheStore;
use Pam\Api\Http\ResponseSnapshot;
use Pam\Contracts\Http\MiddlewareInterface;
use Pam\Contracts\Http\RequestHandlerInterface;
use Pam\Http\Request;
use Pam\Http\Response;

final readonly class ResponseCacheMiddleware implements MiddlewareInterface
{
    /** @var \Closure(Request): string */
    private \Closure $keyResolver;

    public function __construct(
        private ResponseCacheStore $store,
        private int $ttlSeconds = 60,
        ?callable $keyResolver = null,
    ) {
        if ($ttlSeconds < 1) {
            throw new \InvalidArgumentException('Response cache TTL must be positive.');
        }
        $this->keyResolver = $keyResolver === null
            ? static fn (Request $request): string => hash('sha256', $request->method . "\n" . $request->path . "\n" . serialize($request->query()))
            : \Closure::fromCallable($keyResolver);
    }

    public function process(Request $request, Response $response, RequestHandlerInterface $next): Response
    {
        if (!in_array($request->method, ['GET', 'HEAD'], true)) {
            return $next->handle($request, $response);
        }
        $key = ($this->keyResolver)($request);
        if ($key === '') {
            throw new \UnexpectedValueException('Response cache key cannot be empty.');
        }
        $now = time();
        $cached = $this->store->get($key, $now);
        if ($cached !== null) {
            return $cached->response->restore($response)->header('x-cache', 'HIT');
        }
        $result = $next->handle($request, $response);
        $export = $result->export();
        if ($export['status'] >= 200 && $export['status'] < 300) {
            $this->store->put($key, new CacheRecord(ResponseSnapshot::capture($result), $now + $this->ttlSeconds));
        }
        return $result->header('x-cache', 'MISS');
    }
}

