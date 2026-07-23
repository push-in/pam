<?php

declare(strict_types=1);

namespace Pam\Api;

final class Router
{
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
        $method = strtoupper(trim($method));
        if ($method === '' || preg_match('/^[A-Z!#$%&\'*+.^_`|~-]+$/D', $method) !== 1) {
            throw new \InvalidArgumentException('Route method is invalid.');
        }
        if ($path === '' || $path[0] !== '/' || str_contains($path, "\0") || str_contains($path, '?')) {
            throw new \InvalidArgumentException('Route paths must be absolute and cannot contain a query string.');
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
        return $this;
    }

    public function match(string $method, string $path): RoutingResult
    {
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
            return ['#^/$#D', []];
        }

        $names = [];
        $segments = explode('/', ltrim($path, '/'));
        $compiled = [];
        foreach ($segments as $segment) {
            if (preg_match('/^\{([A-Za-z_][A-Za-z0-9_]*)\}$/D', $segment, $match) === 1) {
                $name = $match[1];
                if (in_array($name, $names, true)) {
                    throw new \InvalidArgumentException("Route parameter {$name} is duplicated.");
                }
                $names[] = $name;
                $compiled[] = '(?P<' . $name . '>[^/]+)';
                continue;
            }
            if (str_contains($segment, '{') || str_contains($segment, '}')) {
                throw new \InvalidArgumentException('Route parameters must occupy an entire path segment.');
            }
            $compiled[] = preg_quote($segment, '#');
        }

        return ['#^/' . implode('/', $compiled) . '/?$#D', $names];
    }
}
