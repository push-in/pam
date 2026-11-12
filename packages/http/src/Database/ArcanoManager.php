<?php

declare(strict_types=1);

namespace Pam\Api\Database;

use Arcano\Database\DatabaseInterface;
use Arcano\ORM\EntityManager;
use Arcano\ORM\EntityManagerInterface;
use Arcano\ORM\Factory;
use Arcano\ORM\Mapper\Mapper;
use Arcano\ORM\ORM;
use Arcano\ORM\ORMInterface;
use Arcano\ORM\RepositoryInterface;
use Arcano\ORM\Schema;
use Arcano\ORM\SchemaInterface;
use Pam\Api\Container\Container;
use Pam\Api\Transactions\TransactionManager;

/**
 * Arcano ORM for PAM: every request Fiber gets its own connections and identity map.
 */
final class ArcanoManager implements TransactionManager
{
    /** @var \WeakMap<object, ORMInterface> */
    private \WeakMap $fiberOrms;

    private ?ORMInterface $mainOrm = null;

    private SchemaInterface $schema;

    /** @param SchemaInterface|array<array-key, array<int, mixed>> $schema */
    public function __construct(
        private readonly ArcanoDatabaseProvider $databases,
        SchemaInterface|array $schema,
        private readonly ?Container $container = null,
    ) {
        $this->fiberOrms = new \WeakMap();
        $this->schema = is_array($schema) ? new Schema($schema) : $schema;
    }

    public function database(\UnitEnum|string|null $name = null): DatabaseInterface
    {
        return $this->databases->database($name);
    }

    public function orm(): ORMInterface
    {
        $fiber = \Fiber::getCurrent();
        if ($fiber === null) {
            return $this->mainOrm ??= $this->createOrm();
        }
        return $this->fiberOrms[$fiber] ??= $this->createOrm();
    }

    /**
     * @param class-string|non-empty-string $role
     * @return RepositoryInterface<object>
     */
    public function repository(string $role): RepositoryInterface
    {
        return $this->orm()->getRepository($role);
    }

    /**
     * A new unit of work over the identity map of the current request.
     */
    public function entityManager(): EntityManagerInterface
    {
        return new EntityManager($this->orm());
    }

    public function transaction(callable $operation): mixed
    {
        return $this->database()->transaction(static fn (): mixed => $operation());
    }

    /**
     * Limits every select of the role to the current tenant and stamps the tenant on
     * new entities. Reads and writes fail without an active TenantContext.
     */
    public function protectTenant(string $role, string $column = 'tenant_id'): void
    {
        if ($this->container === null) {
            throw new \LogicException('Tenant protection requires the application container.');
        }
        if (!$this->schema instanceof Schema) {
            throw new \LogicException('Tenant protection requires an Arcano\\ORM\\Schema.');
        }
        $schema = $this->schema->toArray();
        $resolved = $this->schema->resolveAlias($role) ?? $role;
        if (!isset($schema[$resolved])) {
            throw new \InvalidArgumentException("{$role} is not an Arcano role.");
        }
        $mapper = $schema[$resolved][SchemaInterface::MAPPER] ?? Mapper::class;
        if ($mapper !== Mapper::class && $mapper !== ArcanoTenantMapper::class && !is_subclass_of($mapper, ArcanoTenantMapper::class)) {
            throw new \LogicException("The {$role} mapper must extend " . ArcanoTenantMapper::class . '.');
        }
        if ($mapper === Mapper::class) {
            $schema[$resolved][SchemaInterface::MAPPER] = ArcanoTenantMapper::class;
        }
        $schema[$resolved][SchemaInterface::SCOPE] = new ArcanoTenantScope($this->container, $column);
        $this->schema = new Schema($schema);

        // Identity maps built with the previous schema are dropped.
        $this->mainOrm = null;
        $this->fiberOrms = new \WeakMap();
    }

    public function releaseCurrentRequest(): void
    {
        $fiber = \Fiber::getCurrent();
        if ($fiber === null) {
            $this->mainOrm = null;
        } else {
            unset($this->fiberOrms[$fiber]);
        }
        $this->databases->releaseCurrent();
    }

    private function createOrm(): ORMInterface
    {
        return new ORM(new Factory($this->databases->manager()), $this->schema);
    }
}
