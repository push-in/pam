<?php

declare(strict_types=1);

namespace Nimbus\Database;

use Arcano\Database\DatabaseInterface;
use Arcano\Database\DatabaseManager;
use Arcano\ORM\EntityManager;
use Arcano\ORM\EntityManagerInterface;
use Arcano\ORM\Factory;
use Arcano\ORM\Mapper\Mapper;
use Arcano\ORM\ORM;
use Arcano\ORM\ORMInterface;
use Arcano\ORM\RepositoryInterface;
use Arcano\ORM\Schema;
use Arcano\ORM\SchemaInterface;
use Arcano\ORM\Select;
use Nimbus\Container\Container;
use Nimbus\Transactions\TransactionManager;

/**
 * Arcano ORM for PAM: every request Fiber gets its own connections and identity map.
 * The ORM follows its connection set through the worker pool; its identity map is
 * emptied when the request ends.
 */
final class ArcanoManager implements TransactionManager
{
    /** @var \WeakMap<DatabaseManager, ORMInterface> */
    private \WeakMap $orms;

    private SchemaInterface $schema;

    /** @param SchemaInterface|array<array-key, array<int, mixed>> $schema */
    public function __construct(
        private readonly ArcanoDatabaseProvider $databases,
        SchemaInterface|array $schema,
        private readonly ?Container $container = null,
    ) {
        $this->orms = new \WeakMap();
        $this->schema = is_array($schema) ? new Schema($schema) : $schema;
    }

    public function database(\UnitEnum|string|null $name = null): DatabaseInterface
    {
        return $this->databases->database($name);
    }

    public function orm(): ORMInterface
    {
        $manager = $this->databases->manager();
        return $this->orms[$manager] ??= new ORM(new Factory($manager), $this->schema);
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
     * A query over the entities of a role, with relation loading (`load()`), filters and paging.
     *
     * @param class-string|non-empty-string $role
     * @return Select<object>
     */
    public function select(string $role): Select
    {
        return new Select($this->orm(), $role);
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

        // ORMs built with the previous schema are dropped.
        $this->orms = new \WeakMap();
    }

    public function releaseCurrentRequest(): void
    {
        $manager = $this->databases->current();
        if ($manager !== null && isset($this->orms[$manager])) {
            $this->orms[$manager]->getHeap()->clean();
        }
        $this->databases->releaseCurrent();
    }
}
