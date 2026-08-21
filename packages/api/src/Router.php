<?php

declare(strict_types=1);

namespace Pam\Api;

use Pam\Api\Container\Container;

final class Router
{
    private const int MAXIMUM_METHOD_BYTES = 32;
    private const int MAXIMUM_PATH_BYTES = 2_048;
    private const int MAXIMUM_ROUTE_NAME_BYTES = 128;
    private const int MAXIMUM_CONSTRAINT_BYTES = 512;
    private const int MAXIMUM_SEGMENTS = 128;
    private const int MAXIMUM_PARAMETERS = 32;
    private const string PCRE_BUDGET = '(*LIMIT_MATCH=100000)(*LIMIT_DEPTH=1000)';

    public function __construct(
        private readonly ?Container $container = null,
        private readonly int $maximumRoutes = 10_000,
    ) {
        if ($maximumRoutes < 1 || $maximumRoutes > 100_000) {
            throw new \InvalidArgumentException('Maximum routes must be between 1 and 100,000.');
        }
    }

    public function container(): Container
    {
        return $this->container ?? throw new \LogicException('Router has no application container.');
    }
    /** @var list<Route> */
    private array $routes = [];

    /** @var array<string, array<string, Route>> */
    private array $staticRoutes = [];

    /** @var list<Route> */
    private array $dynamicRoutes = [];

    /** @var array<string, true> */
    private array $signatures = [];

    public function add(string $method, string $path, callable $handler): self
    {
        $this->register($method, $path, $handler);
        return $this;
    }

    public function register(string $method, string $path, callable $handler): Route
    {
        $method = strtoupper(trim($method));
        if (
            $method === ''
            || strlen($method) > self::MAXIMUM_METHOD_BYTES
            || preg_match('/^[A-Z!#$%&\'*+.^_`|~-]+$/D', $method) !== 1
        ) {
            throw new \InvalidArgumentException('Route method is invalid.');
        }
        if (
            $path === ''
            || strlen($path) > self::MAXIMUM_PATH_BYTES
            || $path[0] !== '/'
            || str_contains($path, "\0")
            || str_contains($path, '?')
        ) {
            throw new \InvalidArgumentException('Route paths must be absolute and cannot contain a query string.');
        }
        if (count($this->routes) >= $this->maximumRoutes) {
            throw new \OverflowException('Router route limit exceeded.');
        }

        $signature = $method . ' ' . $path;
        if (isset($this->signatures[$signature])) {
            throw new \LogicException("Route {$signature} is already registered.");
        }

        [$pattern, $parameterNames] = self::compile($path);
        $route = new Route($method, $path, $handler, $pattern, $parameterNames);
        $this->routes[] = $route;
        if ($parameterNames === []) {
            $this->staticRoutes[self::normalizeStaticPath($path)][$method] = $route;
        } else {
            $this->dynamicRoutes[] = $route;
        }
        $this->signatures[$signature] = true;
        return $route;
    }

    public function name(Route $route, string $name): void
    {
        if (
            $name === ''
            || strlen($name) > self::MAXIMUM_ROUTE_NAME_BYTES
            || preg_match('/^[A-Za-z0-9_.-]+$/D', $name) !== 1
        ) {
            throw new \InvalidArgumentException('Route names may contain letters, numbers, dots, dashes and underscores.');
        }
        foreach ($this->routes as $registered) {
            if ($registered !== $route && $registered->name === $name) {
                throw new \LogicException("Route name {$name} is already registered.");
            }
        }
        $route->name = $name;
    }

    public function constrain(Route $route, string $parameter, string|RouteConstraint $constraint): void
    {
        if (!in_array($parameter, $route->parameterNames, true)) {
            throw new \InvalidArgumentException("Route {$route->path} has no parameter named {$parameter}.");
        }
        $pattern = $constraint instanceof RouteConstraint ? $constraint->pattern() : $constraint;
        if (
            $pattern === ''
            || strlen($pattern) > self::MAXIMUM_CONSTRAINT_BYTES
            || @preg_match('#' . self::PCRE_BUDGET . '^(?:' . $pattern . ')$#D', '') === false
        ) {
            throw new \InvalidArgumentException("Constraint for {$parameter} is not a valid regular expression.");
        }
        $needle = '(?P<' . $parameter . '>[^/]+)';
        $route->pattern = str_replace($needle, '(?P<' . $parameter . '>' . $pattern . ')', $route->pattern);
    }

    public function match(string $method, string $path): RoutingResult
    {
        if (strlen($method) > self::MAXIMUM_METHOD_BYTES || strlen($path) > self::MAXIMUM_PATH_BYTES) {
            return new RoutingResult(RoutingResultType::NotFound);
        }
        $method = strtoupper($method);
        $allowedMethods = [];

        $static = $this->staticRoutes[self::normalizeStaticPath($path)] ?? [];
        $staticRoute = $static[$method] ?? ($method === 'HEAD' ? ($static['GET'] ?? null) : null);
        if ($staticRoute instanceof Route) {
            return new RoutingResult(RoutingResultType::Found, $staticRoute);
        }
        $allowedMethods = array_keys($static);

        foreach ($this->dynamicRoutes as $route) {
            $matches = [];
            if (preg_match($route->pattern, $path, $matches) !== 1) {
                continue;
            }

            if ($route->method !== $method && !($method === 'HEAD' && $route->method === 'GET')) {
                $allowedMethods[] = $route->method;
                continue;
            }

            $parameters = [];
            foreach ($route->parameterNames as $name) {
                $parameters[$name] = rawurldecode($matches[$name] ?? '');
            }

            return new RoutingResult(RoutingResultType::Found, $route, $parameters);
        }

        if ($allowedMethods !== []) {
            if (in_array('GET', $allowedMethods, true)) {
                $allowedMethods[] = 'HEAD';
            }
            $allowedMethods[] = 'OPTIONS';
            $allowedMethods = array_values(array_unique($allowedMethods));
            sort($allowedMethods);
            return new RoutingResult(
                RoutingResultType::MethodNotAllowed,
                allowedMethods: $allowedMethods,
            );
        }

        return new RoutingResult(RoutingResultType::NotFound);
    }

    private static function normalizeStaticPath(string $path): string
    {
        return $path === '/' ? '/' : rtrim($path, '/');
    }

    /** @return list<Route> */
    public function routes(): array
    {
        return $this->routes;
    }

    /** @return array{string, list<string>} */
    private static function compile(string $path): array
    {
        if ($path === '/') {
            return ['#' . self::PCRE_BUDGET . '^/$#D', []];
        }

        $names = [];
        $segments = explode('/', ltrim($path, '/'));
        if (count($segments) > self::MAXIMUM_SEGMENTS) {
            throw new \InvalidArgumentException('Route path contains too many segments.');
        }
        $compiled = [];
        foreach ($segments as $segment) {
            if (preg_match('/^\{([A-Za-z_][A-Za-z0-9_]*)\}$/D', $segment, $match) === 1) {
                $name = $match[1];
                if (in_array($name, $names, true)) {
                    throw new \InvalidArgumentException("Route parameter {$name} is duplicated.");
                }
                $names[] = $name;
                if (count($names) > self::MAXIMUM_PARAMETERS) {
                    throw new \InvalidArgumentException('Route path contains too many parameters.');
                }
                $compiled[] = '(?P<' . $name . '>[^/]+)';
                continue;
            }
            if (str_contains($segment, '{') || str_contains($segment, '}')) {
                throw new \InvalidArgumentException('Route parameters must occupy an entire path segment.');
            }
            $compiled[] = preg_quote($segment, '#');
        }

        return ['#' . self::PCRE_BUDGET . '^/' . implode('/', $compiled) . '/?$#D', $names];
    }
}
