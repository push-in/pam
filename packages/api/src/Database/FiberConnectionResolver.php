<?php

declare(strict_types=1);

namespace Pam\Api\Database;

use Illuminate\Container\Container as IlluminateContainer;
use Illuminate\Database\Capsule\Manager as Capsule;
use Illuminate\Database\Connection;
use Illuminate\Database\ConnectionResolverInterface;
use Illuminate\Events\Dispatcher;

/**
 * Gives every PAM request Fiber its own Eloquent connection manager.
 *
 * @internal Applications should resolve EloquentManager instead.
 */
final class FiberConnectionResolver implements ConnectionResolverInterface
{
    /** @var \WeakMap<object, Capsule> */
    private \WeakMap $fiberCapsules;

    private ?Capsule $mainCapsule = null;

    private string $defaultConnection;

    private readonly Dispatcher $events;

    public function __construct(private readonly DatabaseConfig $config)
    {
        $this->defaultConnection = $config->defaultConnection;
        $this->fiberCapsules = new \WeakMap();
        $this->events = new Dispatcher(new IlluminateContainer());
    }

    public function connection($name = null): Connection
    {
        return $this->capsule()->getConnection(self::connectionName($name) ?? $this->defaultConnection);
    }

    public function getDefaultConnection(): string
    {
        return $this->defaultConnection;
    }

    public function eventDispatcher(): Dispatcher
    {
        return $this->events;
    }

    public function setDefaultConnection($name): void
    {
        if ($name === '' || !isset($this->config->connections[$name])) {
            throw new \InvalidArgumentException('The Eloquent default connection must be a configured name.');
        }
        $this->defaultConnection = $name;
    }

    public function releaseCurrent(): void
    {
        $fiber = \Fiber::getCurrent();
        $capsule = $fiber === null ? $this->mainCapsule : ($this->fiberCapsules[$fiber] ?? null);
        if ($capsule === null) {
            return;
        }

        foreach (array_keys($this->config->connections) as $name) {
            $manager = $capsule->getDatabaseManager();
            if ($manager->connection($name)->transactionLevel() > 0) {
                $manager->connection($name)->rollBack(0);
            }
            $manager->disconnect($name);
        }

        if ($fiber === null) {
            $this->mainCapsule = null;
            return;
        }
        unset($this->fiberCapsules[$fiber]);
    }

    private function capsule(): Capsule
    {
        $fiber = \Fiber::getCurrent();
        if ($fiber === null) {
            return $this->mainCapsule ??= $this->createCapsule();
        }
        return $this->fiberCapsules[$fiber] ??= $this->createCapsule();
    }

    private function createCapsule(): Capsule
    {
        $container = new IlluminateContainer();
        $capsule = new Capsule($container);
        foreach ($this->config->connections as $name => $connection) {
            $capsule->addConnection($connection, $name);
        }
        $capsule->getDatabaseManager()->setDefaultConnection($this->defaultConnection);
        $capsule->setEventDispatcher($this->events);
        return $capsule;
    }

    private static function connectionName(\UnitEnum|string|null $name): ?string
    {
        if ($name instanceof \BackedEnum) {
            return (string) $name->value;
        }
        if ($name instanceof \UnitEnum) {
            return $name->name;
        }
        return $name;
    }
}
