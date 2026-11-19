<?php

declare(strict_types=1);

namespace Nimbus;

use Nimbus\Container\Container;
use Pam\Http\Request;
use Pam\Http\Response;

final readonly class HandlerResolver
{
    public function __construct(private Container $container)
    {
    }

    /**
     * @param callable|class-string|array{class-string, non-empty-string} $handler
     */
    public function resolve(callable|string|array $handler): \Closure
    {
        if (is_array($handler)) {
            [$controller, $method] = $handler;
            if (is_object($controller)) {
                if (!is_callable($handler)) {
                    throw new \InvalidArgumentException('Object controller handler must be callable.');
                }
                return \Closure::fromCallable($handler);
            }
            if (!class_exists($controller) || !method_exists($controller, $method)) {
                throw new \InvalidArgumentException("Controller handler {$controller}::{$method} does not exist.");
            }
            return (new ControllerHandler($this->container, $controller, $method))(...);
        }
        if (is_string($handler)) {
            if (!class_exists($handler) || !method_exists($handler, '__invoke')) {
                throw new \InvalidArgumentException("Invokable controller {$handler} must define __invoke().");
            }
            return (new ControllerHandler($this->container, $handler, '__invoke'))(...);
        }
        $closure = \Closure::fromCallable($handler);
        if (self::takesRequestAndResponse($closure)) {
            return $closure;
        }
        $container = $this->container;
        return static fn (Request $request, Response $response): mixed => $container->callFunction(
            $closure,
            $request->routeParameters(),
            [$request, $response],
        );
    }

    /** Closures declaring only `($request, $response)` skip the container. */
    private static function takesRequestAndResponse(\Closure $closure): bool
    {
        $parameters = (new \ReflectionFunction($closure))->getParameters();
        if (count($parameters) > 2) {
            return false;
        }
        foreach ($parameters as $position => $parameter) {
            $type = $parameter->getType();
            if ($type === null || ($type instanceof \ReflectionNamedType && $type->getName() === 'mixed')) {
                continue;
            }
            $expected = $position === 0 ? Request::class : Response::class;
            if (!$type instanceof \ReflectionNamedType || $type->getName() !== $expected) {
                return false;
            }
        }
        return true;
    }
}
