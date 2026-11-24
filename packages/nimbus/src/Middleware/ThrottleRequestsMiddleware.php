<?php

declare(strict_types=1);

namespace Nimbus\Middleware;

use Nimbus\Auth\AuthContext;
use Nimbus\Container\Container;
use Nimbus\Http\HttpException;
use Nimbus\Http\ProblemCode;
use Nimbus\RateLimit\Limit;
use Nimbus\RateLimit\RateLimiter;
use Pam\Contracts\Http\MiddlewareInterface;
use Pam\Contracts\Http\RequestHandlerInterface;
use Pam\Http\Request;
use Pam\Http\Response;

/**
 * Laravel's `throttle` middleware: a named limiter (`throttle:buck`, `->rateLimit('buck')`)
 * or an inline limit (`throttle:3,1`, `->throttle(3, 1)`), with fixed windows and the
 * `X-RateLimit-Limit`, `X-RateLimit-Remaining`, `Retry-After` and `X-RateLimit-Reset` headers.
 *
 * A request over the limit raises a 429 `HttpException` rendered by the app's error renderer,
 * unless the limit defines `response()`.
 */
final readonly class ThrottleRequestsMiddleware implements MiddlewareInterface
{
    private function __construct(
        private Container $container,
        private ?string $limiterName,
        private int $maxAttempts,
        private int $decaySeconds,
        private string $prefix,
    ) {
    }

    /** A limiter registered with `RateLimiter::for()`; resolved on each request. */
    public static function named(Container $container, string $name): self
    {
        return new self($container, $name, 0, 0, '');
    }

    /**
     * `$maxAttempts` per `$decayMinutes`, counted per authenticated principal or client IP
     * and per route.
     */
    public static function perMinutes(Container $container, int $maxAttempts, int $decayMinutes = 1, string $prefix = ''): self
    {
        return self::perSeconds($container, $maxAttempts, 60 * $decayMinutes, $prefix);
    }

    public static function perSeconds(Container $container, int $maxAttempts, int $decaySeconds, string $prefix = ''): self
    {
        if ($maxAttempts < 1 || $decaySeconds < 1) {
            throw new \InvalidArgumentException('Throttle needs at least one attempt per window of at least one second.');
        }
        return new self($container, null, $maxAttempts, $decaySeconds, $prefix);
    }

    /** Laravel's alias: `throttle:buck` (named limiter) or `throttle:3,1` (attempts, minutes). */
    public static function fromAlias(Container $container, string $alias): self
    {
        $arguments = explode(',', substr($alias, strlen('throttle:')));
        $first = trim($arguments[0]);
        if ($first === '') {
            throw new \InvalidArgumentException("Throttle alias {$alias} names no limiter.");
        }
        if (!ctype_digit($first)) {
            return self::named($container, $first);
        }
        $minutes = trim($arguments[1] ?? '1');
        if (!ctype_digit($minutes)) {
            throw new \InvalidArgumentException("Throttle alias {$alias} has an invalid decay.");
        }
        return self::perMinutes($container, (int) $first, (int) $minutes, trim($arguments[2] ?? ''));
    }

    public function process(Request $request, Response $response, RequestHandlerInterface $next): Response
    {
        $limiter = $this->container->get(RateLimiter::class);
        if (!$limiter instanceof RateLimiter) {
            throw new \LogicException('The container binds another RateLimiter.');
        }
        $limits = $this->limits($limiter, $request);
        $checked = [];
        foreach ($limits as $index => $limit) {
            if ($limit->isUnlimited()) {
                continue;
            }
            $key = $this->key($request, $limit, $index);
            if ($limiter->tooManyAttempts($key, $limit->maxAttempts)) {
                return $this->tooManyAttempts($limiter, $request, $response, $key, $limit);
            }
            $checked[] = [$key, $limit];
        }
        foreach ($checked as [$key, $limit]) {
            $limiter->hit($key, $limit->decaySeconds);
        }
        $result = $next->handle($request, $response);
        foreach ($checked as [$key, $limit]) {
            $result->header('x-ratelimit-limit', (string) $limit->maxAttempts)
                ->header('x-ratelimit-remaining', (string) $limiter->remaining($key, $limit->maxAttempts));
        }
        return $result;
    }

    /** @return list<Limit> */
    private function limits(RateLimiter $limiter, Request $request): array
    {
        if ($this->limiterName === null) {
            return [new Limit('', $this->maxAttempts, $this->decaySeconds)];
        }
        $callback = $limiter->limiter($this->limiterName)
            ?? throw new \LogicException("Rate limiter [{$this->limiterName}] is not defined.");
        $limits = $callback($request);
        return $limits instanceof Limit ? [$limits] : $limits;
    }

    private function key(Request $request, Limit $limit, int $index): string
    {
        if ($this->limiterName !== null) {
            // Laravel keys a named limit by the limiter name and the limit's by() key.
            return sha1($this->limiterName . ':' . $index . ':' . $limit->key);
        }
        $route = $this->container->scopedValue(\Nimbus\Route::class);
        $signature = $route instanceof \Nimbus\Route ? $route->method . ' ' . $route->path : $request->method . ' ' . $request->path;
        return sha1($this->prefix . '|' . $signature . '|' . $this->requester($request));
    }

    private function requester(Request $request): string
    {
        $auth = $this->container->scopedValue(AuthContext::class);
        if ($auth instanceof AuthContext) {
            return 'principal:' . $auth->principal->identifier();
        }
        $ip = $this->container->has(\Nimbus\Http\ClientIpResolver::class)
            ? $this->container->get(\Nimbus\Http\ClientIpResolver::class)
            : null;
        if ($ip instanceof \Nimbus\Http\ClientIpResolver) {
            return 'ip:' . $ip->resolve($request);
        }
        return 'ip:' . (is_string($_SERVER['REMOTE_ADDR'] ?? null) ? $_SERVER['REMOTE_ADDR'] : 'unknown');
    }

    private function tooManyAttempts(
        RateLimiter $limiter,
        Request $request,
        Response $response,
        string $key,
        Limit $limit,
    ): Response {
        $retryAfter = max(1, $limiter->availableIn($key));
        $headers = [
            'x-ratelimit-limit' => (string) $limit->maxAttempts,
            'x-ratelimit-remaining' => '0',
            'retry-after' => (string) $retryAfter,
            'x-ratelimit-reset' => (string) (time() + $retryAfter),
        ];
        foreach ($headers as $name => $value) {
            $response->header($name, $value);
        }
        if ($limit->responseCallback !== null) {
            return ($limit->responseCallback)($request, $response, $headers);
        }
        throw new HttpException(429, ProblemCode::RateLimited, 'Too Many Requests');
    }
}
