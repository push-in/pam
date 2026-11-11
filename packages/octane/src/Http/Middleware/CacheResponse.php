<?php

declare(strict_types=1);

namespace Pam\Octane\Http\Middleware;

use Closure;
use Illuminate\Cookie\Middleware\AddQueuedCookiesToResponse;
use Illuminate\Cookie\Middleware\EncryptCookies;
use Illuminate\Database\Eloquent\Model;
use Illuminate\Foundation\Http\Middleware\VerifyCsrfToken;
use Illuminate\Http\Request;
use Illuminate\Session\Middleware\StartSession;
use Illuminate\View\Middleware\ShareErrorsFromSession;
use Symfony\Component\HttpFoundation\BinaryFileResponse;
use Symfony\Component\HttpFoundation\Response;
use Symfony\Component\HttpFoundation\StreamedResponse;

/**
 * Marks a response cacheable by PAM's edge page cache, which then serves it
 * from Rust without entering PHP until it expires or is purged.
 *
 *     Route::get('/products/{product}', ShowProduct::class)
 *         ->middleware(CacheResponse::for(ttl: 600, staleWhileRevalidate: 60, tags: ['products', 'product:{product}']))
 *         ->withoutMiddleware(CacheResponse::STATEFUL);
 *
 * Only GET/HEAD responses with a cacheable status and no cookies are marked.
 * `{parameter}` in a tag is replaced by the route parameter (a model's key).
 * PAM never stores a response that sets cookies, so cached routes must not
 * run the session/cookie middleware: remove `CacheResponse::STATEFUL`.
 */
final class CacheResponse
{
    /** Middleware that make a response user-specific (session, CSRF, cookies). */
    public const STATEFUL = [
        StartSession::class,
        ShareErrorsFromSession::class,
        VerifyCsrfToken::class,
        \Illuminate\Foundation\Http\Middleware\ValidateCsrfToken::class,
        AddQueuedCookiesToResponse::class,
        EncryptCookies::class,
    ];

    private const CACHEABLE_STATUSES = [200, 203, 204, 300, 301, 308, 404, 410];

    /** @param list<string> $tags */
    public static function for(int $ttl, int $staleWhileRevalidate = 0, array $tags = []): string
    {
        foreach ($tags as $tag) {
            if (str_contains($tag, ',')) {
                throw new \InvalidArgumentException('PAM cache tags cannot contain commas.');
            }
        }

        return self::class.':'.implode(',', [$ttl, $staleWhileRevalidate, ...$tags]);
    }

    public function handle(
        Request $request,
        Closure $next,
        string|int $ttl = 60,
        string|int $staleWhileRevalidate = 0,
        string ...$tags,
    ): mixed {
        $response = $next($request);
        if (!$response instanceof Response || !$request->isMethodCacheable()) {
            return $response;
        }
        $ttl = (int) $ttl;
        $staleWhileRevalidate = max(0, (int) $staleWhileRevalidate);
        if ($ttl < 1
            || !in_array($response->getStatusCode(), self::CACHEABLE_STATUSES, true)
            || $response->headers->getCookies() !== []
            || $response->headers->has('Set-Cookie')
            || $response instanceof StreamedResponse
            || $response instanceof BinaryFileResponse
            || self::explicitlyPersonal($response)) {
            return $response;
        }

        $directives = "public, max-age=0, s-maxage={$ttl}";
        if ($staleWhileRevalidate > 0) {
            $directives .= ", stale-while-revalidate={$staleWhileRevalidate}";
        }
        $response->headers->set('Cache-Control', $directives);

        $resolved = $this->tags($request, $tags);
        if ($resolved !== []) {
            $response->headers->set('Pam-Cache-Tags', implode(',', $resolved));
        }

        return $response;
    }

    /**
     * Whether the application itself declared the response personal: Symfony
     * reports `no-cache, private` for every response without Cache-Control,
     * so only directives the application set count.
     */
    private static function explicitlyPersonal(Response $response): bool
    {
        /** @var array<string, mixed> $explicit */
        $explicit = (fn (): array => $this->cacheControl)->call($response->headers);

        return isset($explicit['private']) || isset($explicit['no-store']) || isset($explicit['no-cache']);
    }

    /**
     * @param array<array-key, string> $tags
     * @return list<string>
     */
    private function tags(Request $request, array $tags): array
    {
        $route = $request->route();
        $resolved = [];
        foreach ($tags as $tag) {
            $tag = (string) preg_replace_callback('/\{(\w+)\}/', static function (array $match) use ($route): string {
                $value = is_object($route) && method_exists($route, 'parameter') ? $route->parameter($match[1]) : null;
                if ($value instanceof Model) {
                    $value = $value->getKey();
                }

                return is_scalar($value) ? (string) $value : $match[0];
            }, $tag);
            if (preg_match('/\A[A-Za-z0-9\-_.:\/]{1,128}\z/', $tag) === 1) {
                $resolved[] = $tag;
            }
        }

        return array_values(array_unique($resolved));
    }
}
