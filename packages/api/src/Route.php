<?php

declare(strict_types=1);

namespace Pam\Api;

use Pam\Contracts\Http\MiddlewareInterface;

final class Route
{
    public \Closure $handler;

    /** @var list<MiddlewareInterface|callable> */
    public array $middleware = [];

    public ?string $name = null;

    public ?string $summary = null;

    /** @var list<string> */
    public array $tags = [];

    /** @var class-string|null */
    public ?string $input = null;

    /** @var class-string|null */
    public ?string $output = null;

    /** @var callable|class-string|array{class-string, non-empty-string}|null */
    public mixed $sourceHandler = null;

    /** @param list<string> $parameterNames */
    public function __construct(
        public string $method,
        public string $path,
        callable $handler,
        public string $pattern,
        public readonly array $parameterNames,
    ) {
        $this->handler = \Closure::fromCallable($handler);
    }
}
