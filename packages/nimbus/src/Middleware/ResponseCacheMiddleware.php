<?php

declare(strict_types=1);

namespace Nimbus\Middleware;

use Nimbus\Cache\CacheRecord;
use Nimbus\Cache\ResponseCacheStore;
use Nimbus\Http\ResponseSnapshot;
use Pam\Contracts\Http\MiddlewareInterface;
use Pam\Contracts\Http\RequestHandlerInterface;
use Pam\Http\Request;
use Pam\Http\Response;

/**
 * Caches successful GET/HEAD responses for `$ttlSeconds`.
 *
 * The default key is the method, path and query plus a digest of the request's credentials
 * (`Authorization` and `Cookie`), so a signed-in user is never served another user's response.
 * Responses that set a cookie, or that are `Cache-Control: private`/`no-store`/`no-cache`, or
 * `Vary: *`, are never stored. A custom `$keyResolver` replaces the whole key.
 */
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
            ? static fn (Request $request): string => hash('sha256', $request->method . "\n" . $request->path
                . "\n" . serialize($request->query())
                . "\n" . hash('sha256', ($request->getHeader('authorization') ?? '') . "\n" . ($request->getHeader('cookie') ?? '')))
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
        if ($export['status'] >= 200 && $export['status'] < 300 && self::storable($export['headers'])) {
            $this->store->put($key, new CacheRecord(ResponseSnapshot::capture($result), $now + $this->ttlSeconds));
        }
        return $result->header('x-cache', 'MISS');
    }

    /** @param array<string, list<string>> $headers */
    private static function storable(array $headers): bool
    {
        $headers = array_change_key_case($headers);
        if (($headers['set-cookie'] ?? []) !== []) {
            return false;
        }
        $cacheControl = strtolower(implode(',', $headers['cache-control'] ?? []));
        if (preg_match('/(^|[\s,])(private|no-store|no-cache)\b/', $cacheControl) === 1) {
            return false;
        }
        foreach ($headers['vary'] ?? [] as $vary) {
            if (trim($vary) === '*') {
                return false;
            }
        }
        return true;
    }
}
