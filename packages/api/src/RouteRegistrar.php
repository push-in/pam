<?php

declare(strict_types=1);

namespace Pam\Api;

use Pam\App;
use Pam\Contracts\Http\MiddlewareInterface;

final class RouteRegistrar
{
    /** @var list<MiddlewareInterface|callable> */
    private array $middleware = [];

    public function __construct(
        private readonly App $app,
        private string $prefix = '',
    ) {
    }

    public function prefix(string $prefix): self
    {
        $clone = clone $this;
        $clone->prefix = self::join($clone->prefix, $prefix);
        return $clone;
    }

    /** @param MiddlewareInterface|callable|class-string<MiddlewareInterface>|list<MiddlewareInterface|callable|class-string<MiddlewareInterface>> $middleware */
    public function middleware(MiddlewareInterface|callable|string|array $middleware): self
    {
        $clone = clone $this;
        foreach (is_array($middleware) ? $middleware : [$middleware] as $layer) {
            if (is_string($layer) && is_a($layer, MiddlewareInterface::class, true)) {
                $layer = new ContainerMiddleware($this->app->container(), $layer);
            }
            if (!$layer instanceof MiddlewareInterface && !is_callable($layer)) {
                throw new \InvalidArgumentException('Group middleware must implement the PAM contract or be callable.');
            }
            $clone->middleware[] = $layer;
        }
        return $clone;
    }

    /** @param callable(self): void $routes */
    public function group(callable $routes): void
    {
        $routes($this);
    }

    /** @param callable|class-string|array{class-string, non-empty-string} $handler */
    public function get(string $path, callable|string|array $handler): PendingRoute
    {
        return $this->decorate($this->app->get(self::join($this->prefix, $path), $handler));
    }

    /** @param callable|class-string|array{class-string, non-empty-string} $handler */
    public function post(string $path, callable|string|array $handler): PendingRoute
    {
        return $this->decorate($this->app->post(self::join($this->prefix, $path), $handler));
    }

    /** @param callable|class-string|array{class-string, non-empty-string} $handler */
    public function put(string $path, callable|string|array $handler): PendingRoute
    {
        return $this->decorate($this->app->put(self::join($this->prefix, $path), $handler));
    }

    /** @param callable|class-string|array{class-string, non-empty-string} $handler */
    public function patch(string $path, callable|string|array $handler): PendingRoute
    {
        return $this->decorate($this->app->patch(self::join($this->prefix, $path), $handler));
    }

    /** @param callable|class-string|array{class-string, non-empty-string} $handler */
    public function delete(string $path, callable|string|array $handler): PendingRoute
    {
        return $this->decorate($this->app->delete(self::join($this->prefix, $path), $handler));
    }

    /** @param class-string $controller */
    public function apiResource(string $path, string $controller): void
    {
        $base = '/' . trim($path, '/');
        $parameter = self::singularParameter($base);
        $this->get($base, [$controller, 'index']);
        $this->post($base, [$controller, 'store']);
        $this->get("{$base}/{{$parameter}}", [$controller, 'show']);
        $this->put("{$base}/{{$parameter}}", [$controller, 'update']);
        $this->patch("{$base}/{{$parameter}}", [$controller, 'update']);
        $this->delete("{$base}/{{$parameter}}", [$controller, 'destroy']);
    }

    private function decorate(PendingRoute $route): PendingRoute
    {
        foreach ($this->middleware as $middleware) {
            $route->middleware($middleware);
        }
        return $route;
    }

    private static function join(string $prefix, string $path): string
    {
        $joined = '/' . trim($prefix, '/') . '/' . trim($path, '/');
        $joined = preg_replace('#/+#', '/', $joined);
        return $joined === null ? '/' : (rtrim($joined, '/') ?: '/');
    }

    private static function singularParameter(string $path): string
    {
        $segment = basename($path);
        $singular = str_ends_with($segment, 'ies')
            ? substr($segment, 0, -3) . 'y'
            : (str_ends_with($segment, 's') ? substr($segment, 0, -1) : $segment);
        return preg_replace('/[^A-Za-z0-9_]/', '', $singular) ?: 'resource';
    }
}
