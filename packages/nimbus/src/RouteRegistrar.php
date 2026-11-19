<?php

declare(strict_types=1);

namespace Nimbus;

use Nimbus\App;
use Pam\Contracts\Http\MiddlewareInterface;

final class RouteRegistrar
{
    /** @var list<MiddlewareInterface|callable> */
    private array $middleware = [];

    /** @var ?class-string */
    private ?string $controller = null;

    private bool $authenticated = false;

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

    /** Routes registered through the returned registrar require authentication. */
    public function auth(): self
    {
        return $this->authenticated ? $this : $this->withAuthentication();
    }

    /** Routes registered through the returned registrar require the ability; implies `auth()`. */
    public function can(string $ability): self
    {
        return $this->auth()->middleware(new Middleware\AuthorizeMiddleware($this->app->container(), $ability));
    }

    /** Routes registered through the returned registrar require a valid URL signature. */
    public function signed(): self
    {
        return $this->middleware(Middleware\ValidateSignatureMiddleware::class);
    }

    private function withAuthentication(): self
    {
        $clone = $this->middleware(Middleware\AuthenticateMiddleware::class);
        $clone->authenticated = true;
        return $clone;
    }

    /** @param callable(self): void $routes */
    public function group(callable $routes): void
    {
        $routes($this);
    }

    /**
     * Routes declared in the closure name only the controller method:
     * `$router->controller(ProductController::class, function ($router) { $router->get('products', 'index'); })`.
     *
     * @param class-string $controller
     * @param callable(self): void $routes
     */
    public function controller(string $controller, callable $routes): void
    {
        if (!class_exists($controller)) {
            throw new \InvalidArgumentException("Controller {$controller} does not exist.");
        }
        $clone = clone $this;
        $clone->controller = $controller;
        $routes($clone);
    }

    /**
     * Registers index, store, show, update and destroy; the closure adds extra routes
     * relative to the resource path, bound to the same controller.
     *
     * @param class-string $controller
     * @param ?callable(self): void $routes
     */
    public function resource(string $path, string $controller, ?callable $routes = null): void
    {
        $this->apiResource($path, $controller);
        if ($routes !== null) {
            $this->prefix($path)->controller($controller, $routes);
        }
    }

    /** Loads a routes file; it sees this registrar as `$router`. */
    public function load(string $file): void
    {
        if (!is_file($file)) {
            throw new \InvalidArgumentException("Routes file {$file} does not exist.");
        }
        (static function (self $router, string $file): void {
            require $file;
        })($this, $file);
    }

    /** @param callable|class-string|array{class-string, non-empty-string}|non-empty-string $handler Inside controller(), the method name. */
    public function get(string $path, callable|string|array $handler): PendingRoute
    {
        return $this->decorate($this->app->get(self::join($this->prefix, $path), $this->handler($handler)));
    }

    /** @param callable|class-string|array{class-string, non-empty-string}|non-empty-string $handler Inside controller(), the method name. */
    public function post(string $path, callable|string|array $handler): PendingRoute
    {
        return $this->decorate($this->app->post(self::join($this->prefix, $path), $this->handler($handler)));
    }

    /** @param callable|class-string|array{class-string, non-empty-string}|non-empty-string $handler Inside controller(), the method name. */
    public function put(string $path, callable|string|array $handler): PendingRoute
    {
        return $this->decorate($this->app->put(self::join($this->prefix, $path), $this->handler($handler)));
    }

    /** @param callable|class-string|array{class-string, non-empty-string}|non-empty-string $handler Inside controller(), the method name. */
    public function patch(string $path, callable|string|array $handler): PendingRoute
    {
        return $this->decorate($this->app->patch(self::join($this->prefix, $path), $this->handler($handler)));
    }

    /** @param callable|class-string|array{class-string, non-empty-string}|non-empty-string $handler Inside controller(), the method name. */
    public function delete(string $path, callable|string|array $handler): PendingRoute
    {
        return $this->decorate($this->app->delete(self::join($this->prefix, $path), $this->handler($handler)));
    }

    /** @param callable|class-string|array{class-string, non-empty-string}|non-empty-string $handler Inside controller(), the method name. */
    public function head(string $path, callable|string|array $handler): PendingRoute
    {
        return $this->decorate($this->app->head(self::join($this->prefix, $path), $this->handler($handler)));
    }

    /** @param callable|class-string|array{class-string, non-empty-string}|non-empty-string $handler Inside controller(), the method name. */
    public function options(string $path, callable|string|array $handler): PendingRoute
    {
        return $this->decorate($this->app->options(self::join($this->prefix, $path), $this->handler($handler)));
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

    /**
     * @param callable|class-string|array{class-string, non-empty-string}|string $handler
     * @return callable|class-string|array{class-string, non-empty-string}
     */
    private function handler(callable|string|array $handler): callable|string|array
    {
        if (!is_string($handler) || is_callable($handler) || class_exists($handler)) {
            return $handler;
        }
        if ($this->controller === null || $handler === '') {
            throw new \InvalidArgumentException("Route handler {$handler} is not a class; name a method only inside controller().");
        }
        return [$this->controller, $handler];
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
