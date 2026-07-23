<?php

declare(strict_types=1);

namespace Pam\Api;

final readonly class Route
{
    public \Closure $handler;

    /** @param list<string> $parameterNames */
    public function __construct(
        public string $method,
        public string $path,
        callable $handler,
        public string $pattern,
        public array $parameterNames,
    ) {
        $this->handler = \Closure::fromCallable($handler);
    }
}
