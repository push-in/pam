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
use Arcano\ORM\Select\Repository;
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

    /** @var \WeakMap<ORMInterface, array<string, Repository<object>>> Select prototypes by role. */
    private \WeakMap $selects;

    private SchemaInterface $schema;

    /** @param SchemaInterface|array<array-key, array<int, mixed>> $schema */
    public function __construct(
        private readonly ArcanoDatabaseProvider $databases,
        SchemaInterface|array $schema,
        private readonly ?Container $container = null,
    ) {
        $this->orms = new \WeakMap();
        $this->selects = new \WeakMap();
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
        $orm = $this->orm();
        if ($this->schema->define($this->schema->resolveAlias($role) ?? $role, SchemaInterface::SCOPE) !== null) {
            // Scoped roles (tenants, soft deletes) build their query from scratch every time.
            return new Select($orm, $role);
        }
        // Clones of one prototype per ORM share its loader until they change it and the cached
        // statements of their query shapes, so a repeated query skips the loader build and compile.
        $prototypes = $this->selects[$orm] ?? [];
        if (!isset($prototypes[$role])) {
            $prototypes[$role] = new Repository(new Select($orm, $role));
            $this->selects[$orm] = $prototypes;
        }

        return $prototypes[$role]->select();
    }

    /**
     * A new unit of work over the identity map of the current request.
     */
    public function entityManager(): EntityManagerInterface
    {
        return new EntityManager($this->orm());
    }

    /**
     * The entity matching `$attributes`, or a new one with `$attributes + $values` inserted
     * now (Laravel `firstOrCreate`). Keys are entity field names.
     *
     * Concurrent callers are safe when a unique index covers `$attributes`: the insert that
     * loses the race fails on the index (inside a savepoint when a transaction is open) and the
     * row the winner wrote is returned, as Laravel's `createOrFirst()` does.
     *
     * @template T of object
     * @param class-string<T>|non-empty-string $role
     * @param non-empty-array<string, mixed> $attributes
     * @param array<string, mixed> $values
     * @return ($role is class-string<T> ? T : object)
     */
    public function firstOrCreate(string $role, array $attributes, array $values = []): object
    {
        return $this->first($role, $attributes) ?? $this->createOrFirst($role, $attributes, $values);
    }

    /**
     * Inserts `$attributes + $values`; when a unique index rejects it, returns the existing
     * row matching `$attributes` instead (Laravel `createOrFirst`).
     *
     * @template T of object
     * @param class-string<T>|non-empty-string $role
     * @param non-empty-array<string, mixed> $attributes
     * @param array<string, mixed> $values
     * @return ($role is class-string<T> ? T : object)
     */
    public function createOrFirst(string $role, array $attributes, array $values = []): object
    {
        $orm = $this->orm();
        $entity = $orm->make($role, [...$attributes, ...$values]);
        try {
            $this->entityManager()->persist($entity)->run();
            return $entity;
        } catch (\Throwable $error) {
            if (!self::violatesConstraint($error)) {
                throw $error;
            }
            $orm->getHeap()->detach($entity);
            return $this->first($role, $attributes) ?? throw $error;
        }
    }

    /**
     * Updates the entity matching `$attributes` with `$values`, or inserts both (Laravel
     * `updateOrCreate`); race-safe under a unique index on `$attributes` like `firstOrCreate()`.
     *
     * @template T of object
     * @param class-string<T>|non-empty-string $role
     * @param non-empty-array<string, mixed> $attributes
     * @param array<string, mixed> $values
     * @return ($role is class-string<T> ? T : object)
     */
    public function updateOrCreate(string $role, array $attributes, array $values = []): object
    {
        $entity = $this->first($role, $attributes);
        if ($entity === null) {
            $entity = $this->createOrFirst($role, $attributes, $values);
            // A row another worker inserted first still receives the values.
            if ($this->matches($entity, $values)) {
                return $entity;
            }
        }
        if ($values === []) {
            return $entity;
        }
        $this->orm()->getMapper($entity)->hydrate($entity, $values);
        $this->entityManager()->persist($entity)->run();
        return $entity;
    }

    /**
     * @param class-string|non-empty-string $role
     * @param array<string, mixed> $attributes
     */
    private function first(string $role, array $attributes): ?object
    {
        if ($attributes === []) {
            throw new \InvalidArgumentException('firstOrCreate and updateOrCreate need the attributes to match.');
        }
        return $this->select($role)->where($attributes)->fetchOne();
    }

    /** @param array<string, mixed> $values */
    private function matches(object $entity, array $values): bool
    {
        $current = $this->orm()->getMapper($entity)->extract($entity);
        foreach ($values as $field => $value) {
            if (!array_key_exists($field, $current) || $current[$field] != $value) {
                return false;
            }
        }
        return true;
    }

    private static function violatesConstraint(\Throwable $error): bool
    {
        for ($cause = $error; $cause !== null; $cause = $cause->getPrevious()) {
            if ($cause instanceof \Arcano\Database\Exception\StatementException\ConstrainException) {
                return true;
            }
        }
        return false;
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
        $this->selects = new \WeakMap();
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
