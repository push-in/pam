<?php

declare(strict_types=1);

namespace Pam\Api\Database;

/** @internal Fiber-aware facade target for Laravel-style DB calls. */
final readonly class FiberDatabaseProxy
{
    public function __construct(private FiberConnectionResolver $connections)
    {
    }

    /** @param list<mixed> $arguments */
    public function __call(string $method, array $arguments): mixed
    {
        return $this->connections->connection()->{$method}(...$arguments);
    }

    public function connection(\UnitEnum|string|null $name = null): \Illuminate\Database\Connection
    {
        return $this->connections->connection($name);
    }
}
