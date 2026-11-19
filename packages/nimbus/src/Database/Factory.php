<?php

declare(strict_types=1);

namespace Nimbus\Database;

use Faker\Generator;

/**
 * Builds Arcano entities for tests and seeders. Subclasses name the entity class and
 * return its default attributes from definition(); `count()`, `state()` and `sequence()`
 * return adjusted copies. Attributes named like constructor parameters are passed to the
 * constructor; the others are set as properties. Values may be closures receiving the
 * attributes, or factories, which make the related entity (persisted by cascade on create()).
 *
 * Fake data comes from fakerphp/faker through `$this->faker`.
 *
 * @template TEntity of object
 */
abstract class Factory
{
    /** @var class-string<TEntity> */
    protected string $model;

    protected Generator $faker;

    private static ?ArcanoManager $manager = null;

    private static ?Generator $sharedFaker = null;

    private ?int $count = null;

    /** @var list<array<string, mixed>|\Closure(array<string, mixed>): array<string, mixed>> */
    private array $states = [];

    /** @var list<list<array<string, mixed>|\Closure(int): array<string, mixed>>> */
    private array $sequences = [];

    /** @var list<\Closure(TEntity): void> */
    private array $afterMaking = [];

    /** @var list<\Closure(TEntity): void> */
    private array $afterCreating = [];

    final public function __construct()
    {
        $this->faker = self::faker();
    }

    public static function new(): static
    {
        /** @phpstan-ignore return.type (static carries the subclass's TEntity) */
        return new static();
    }

    /**
     * Sets the manager create() persists through and returns the previous one;
     * Nimbus\Testing\TestCase and SeederRunner set it.
     */
    public static function useManager(?ArcanoManager $manager): ?ArcanoManager
    {
        $previous = self::$manager;
        self::$manager = $manager;
        return $previous;
    }

    /** Replaces the shared Faker generator, e.g. for another locale or a fixed seed. */
    public static function useFaker(?Generator $faker): void
    {
        self::$sharedFaker = $faker;
    }

    /** @return array<string, mixed> */
    abstract public function definition(): array;

    /** @return static */
    public function count(int $count): static
    {
        if ($count < 0) {
            throw new \InvalidArgumentException('A factory count cannot be negative.');
        }
        $clone = clone $this;
        $clone->count = $count;
        return $clone;
    }

    /**
     * @param array<string, mixed>|\Closure(array<string, mixed>): array<string, mixed> $state
     * @return static
     */
    public function state(array|\Closure $state): static
    {
        $clone = clone $this;
        $clone->states[] = $state;
        return $clone;
    }

    /**
     * Applies the states in turn: the first entity gets the first state, and so on, cycling.
     * A closure state receives the entity's index.
     *
     * @param array<string, mixed>|\Closure(int): array<string, mixed> ...$states
     * @return static
     */
    public function sequence(array|\Closure ...$states): static
    {
        if ($states === []) {
            throw new \InvalidArgumentException('A sequence needs at least one state.');
        }
        $clone = clone $this;
        $clone->sequences[] = array_values($states);
        return $clone;
    }

    /**
     * @param \Closure(TEntity): void $callback
     * @return static
     */
    public function afterMaking(\Closure $callback): static
    {
        $clone = clone $this;
        $clone->afterMaking[] = $callback;
        return $clone;
    }

    /**
     * @param \Closure(TEntity): void $callback
     * @return static
     */
    public function afterCreating(\Closure $callback): static
    {
        $clone = clone $this;
        $clone->afterCreating[] = $callback;
        return $clone;
    }

    /**
     * The resolved attributes, without building entities: one set, or a list after count().
     *
     * @param array<string, mixed> $attributes
     * @return array<string, mixed>|list<array<string, mixed>>
     */
    public function raw(array $attributes = []): array
    {
        if ($this->count === null) {
            return $this->attributes(0, $attributes, false);
        }
        $raw = [];
        for ($index = 0; $index < $this->count; ++$index) {
            $raw[] = $this->attributes($index, $attributes, false);
        }
        return $raw;
    }

    /**
     * Unsaved entities: one, or a list after count().
     *
     * @param array<string, mixed> $attributes
     * @return TEntity|list<TEntity>
     */
    public function make(array $attributes = []): object|array
    {
        return $this->count === null ? $this->makeOne($attributes) : $this->makeMany($attributes);
    }

    /**
     * @param array<string, mixed> $attributes
     * @return TEntity
     */
    public function makeOne(array $attributes = []): object
    {
        return $this->build(0, $attributes);
    }

    /**
     * @param array<string, mixed> $attributes
     * @return list<TEntity>
     */
    public function makeMany(array $attributes = []): array
    {
        $entities = [];
        for ($index = 0; $index < ($this->count ?? 1); ++$index) {
            $entities[] = $this->build($index, $attributes);
        }
        return $entities;
    }

    /**
     * Persisted entities, in one unit of work: one, or a list after count().
     *
     * @param array<string, mixed> $attributes
     * @return TEntity|list<TEntity>
     */
    public function create(array $attributes = []): object|array
    {
        return $this->count === null ? $this->createOne($attributes) : $this->createMany($attributes);
    }

    /**
     * @param array<string, mixed> $attributes
     * @return TEntity
     */
    public function createOne(array $attributes = []): object
    {
        return $this->persist([$this->build(0, $attributes)])[0];
    }

    /**
     * @param array<string, mixed> $attributes
     * @return list<TEntity>
     */
    public function createMany(array $attributes = []): array
    {
        return $this->persist($this->makeMany($attributes));
    }

    /**
     * @param list<TEntity> $entities
     * @return list<TEntity>
     */
    private function persist(array $entities): array
    {
        $manager = self::$manager
            ?? throw new \LogicException('Factory::create() needs an ArcanoManager: call Factory::useManager().');
        $unit = $manager->entityManager();
        foreach ($entities as $entity) {
            $unit->persist($entity);
        }
        $unit->run();
        foreach ($entities as $entity) {
            foreach ($this->afterCreating as $callback) {
                $callback($entity);
            }
        }
        return $entities;
    }

    /**
     * @param array<string, mixed> $attributes
     * @return TEntity
     */
    private function build(int $index, array $attributes): object
    {
        $attributes = $this->attributes($index, $attributes, true);
        $entity = self::instantiate($this->model, $attributes);
        $reflection = new \ReflectionObject($entity);
        foreach ($attributes as $name => $value) {
            if (!$reflection->hasProperty($name)) {
                throw new \InvalidArgumentException("{$this->model} has no {$name} property.");
            }
            $reflection->getProperty($name)->setValue($entity, $value);
        }
        foreach ($this->afterMaking as $callback) {
            $callback($entity);
        }
        return $entity;
    }

    /**
     * @param array<string, mixed> $overrides
     * @return array<string, mixed>
     */
    private function attributes(int $index, array $overrides, bool $entities): array
    {
        $attributes = $this->definition();
        foreach ($this->states as $state) {
            $attributes = array_replace($attributes, $state instanceof \Closure ? $state($attributes) : $state);
        }
        foreach ($this->sequences as $sequence) {
            $state = $sequence[$index % count($sequence)];
            $attributes = array_replace($attributes, $state instanceof \Closure ? $state($index) : $state);
        }
        $attributes = array_replace($attributes, $overrides);

        foreach ($attributes as $name => $value) {
            if ($value instanceof self) {
                $attributes[$name] = $entities ? $value->makeOne() : $value->raw();
            }
        }
        foreach ($attributes as $name => $value) {
            if ($value instanceof \Closure) {
                $attributes[$name] = $value($attributes);
            }
        }
        return $attributes;
    }

    /**
     * Calls the constructor with the attributes named like its parameters, which are then
     * removed; without every required one, the entity is built without its constructor.
     *
     * @template T of object
     * @param class-string<T> $class
     * @param array<string, mixed> $attributes
     * @return T
     */
    private static function instantiate(string $class, array &$attributes): object
    {
        $reflection = new \ReflectionClass($class);
        $constructor = $reflection->getConstructor();
        if ($constructor === null) {
            return $reflection->newInstance();
        }
        $arguments = [];
        foreach ($constructor->getParameters() as $parameter) {
            $name = $parameter->getName();
            if (array_key_exists($name, $attributes)) {
                $arguments[$name] = $attributes[$name];
            } elseif (!$parameter->isOptional()) {
                return $reflection->newInstanceWithoutConstructor();
            }
        }
        $attributes = array_diff_key($attributes, $arguments);
        return $reflection->newInstanceArgs($arguments);
    }

    private static function faker(): Generator
    {
        if (self::$sharedFaker !== null) {
            return self::$sharedFaker;
        }
        if (!class_exists(\Faker\Factory::class)) {
            throw new \LogicException('Nimbus factories need fakerphp/faker: composer require --dev fakerphp/faker.');
        }
        return self::$sharedFaker = \Faker\Factory::create();
    }
}
