<?php

declare(strict_types=1);

namespace Pam\Api\Database;

/** @internal Fiber-aware facade target for Laravel-style Schema migrations. */
final readonly class FiberSchemaProxy
{
    public function __construct(private FiberConnectionResolver $connections)
    {
    }

    /** @param list<mixed> $arguments */
    public function __call(string $method, array $arguments): mixed
    {
        return $this->connections->connection()->getSchemaBuilder()->{$method}(...$arguments);
    }
}
