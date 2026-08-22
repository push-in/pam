<?php

declare(strict_types=1);

namespace Pam\Api;

use Pam\Api\Container\Container;

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
        return \Closure::fromCallable($handler);
    }
}
