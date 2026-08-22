<?php

declare(strict_types=1);

namespace Pam\Api;

use Pam\Api\Container\Container;
use Pam\Http\Request;
use Pam\Http\Response;

final readonly class ControllerHandler
{
    /** @param class-string $controller */
    public function __construct(
        private Container $container,
        private string $controller,
        private string $method,
    ) {
        $reflection = new \ReflectionMethod($controller, $method);
        if (!$reflection->isPublic() || $reflection->isAbstract()) {
            throw new \InvalidArgumentException("Controller handler {$controller}::{$method} must be public and concrete.");
        }
    }

    public function __invoke(Request $request, Response $response): mixed
    {
        $controller = $this->container->get($this->controller);
        if (!is_object($controller)) {
            throw new \UnexpectedValueException("Container entry {$this->controller} must resolve to an object.");
        }
        return $this->container->call(
            [$controller, $this->method],
            $request->routeParameters(),
            [$request, $response],
        );
    }
}

