<?php

declare(strict_types=1);

namespace Pam\Api\Database;

use Illuminate\Database\Connection;
use Illuminate\Database\Eloquent\Model;
use Illuminate\Database\Schema\Builder;
use Illuminate\Events\Dispatcher;
use Illuminate\Support\Facades\DB;
use Illuminate\Support\Facades\Schema;
use Pam\Api\Transactions\TransactionManager;

final readonly class EloquentManager implements TransactionManager
{
    public function __construct(private FiberConnectionResolver $connections)
    {
    }

    public function boot(): void
    {
        Model::setConnectionResolver($this->connections);
        Model::setEventDispatcher($this->connections->eventDispatcher());
        DB::swap(new FiberDatabaseProxy($this->connections));
        Schema::swap(new FiberSchemaProxy($this->connections));
    }

    public function connection(?string $name = null): Connection
    {
        return $this->connections->connection($name);
    }

    public function schema(?string $name = null): Builder
    {
        return $this->connection($name)->getSchemaBuilder();
    }

    public function events(): Dispatcher
    {
        return $this->connections->eventDispatcher();
    }

    public function transaction(callable $operation): mixed
    {
        return $this->connection()->transaction(static fn (): mixed => $operation());
    }

    public function releaseCurrentRequest(): void
    {
        $this->connections->releaseCurrent();
    }
}
