<?php

declare(strict_types=1);

namespace Nimbus\Database;

use Nimbus\Container\Container;

/**
 * Fills the database. Subclasses define `run()`; seeders are built and run through the
 * container, so the constructor and run() may ask for services such as ArcanoManager.
 * call() runs other seeders.
 */
abstract class Seeder
{
    private ?Container $container = null;

    /** @var list<class-string<Seeder>> */
    private array $ran = [];

    /**
     * Runs the seeders in order.
     *
     * @param class-string<Seeder>|list<class-string<Seeder>> $seeders
     */
    public function call(string|array $seeders): void
    {
        foreach (is_string($seeders) ? [$seeders] : $seeders as $class) {
            $seeder = self::resolve($class, $this->container);
            $seeder->container = $this->container;
            $seeder->invoke();
            $this->ran = [...$this->ran, ...$seeder->ran];
        }
    }

    /**
     * Runs the seeder and returns the classes that ran, nested ones included, in order.
     *
     * @internal Use SeederRunner.
     * @return list<class-string<Seeder>>
     */
    final public function invoke(?Container $container = null): array
    {
        $this->container = $container ?? $this->container;
        $this->ran = [static::class];
        if (!method_exists($this, 'run')) {
            throw new \LogicException(static::class . ' must define run().');
        }
        if ($this->container !== null) {
            $this->container->call([$this, 'run']);
        } else {
            $this->{'run'}();
        }
        return $this->ran;
    }

    /** @param class-string<Seeder> $class */
    private static function resolve(string $class, ?Container $container): self
    {
        $seeder = $container !== null ? $container->get($class) : new $class();
        if (!$seeder instanceof self) {
            throw new \InvalidArgumentException("{$class} is not a Nimbus\\Database\\Seeder.");
        }
        return $seeder;
    }
}
