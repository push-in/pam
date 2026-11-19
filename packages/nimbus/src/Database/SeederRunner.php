<?php

declare(strict_types=1);

namespace Nimbus\Database;

use Nimbus\App;

/**
 * Runs seeders against a booted application: seeders resolve through its container and
 * factories persist through its ArcanoManager.
 */
final readonly class SeederRunner
{
    public function __construct(private App $app)
    {
    }

    /**
     * @param class-string<Seeder> $seeder
     * @return list<class-string<Seeder>> The seeders that ran, in order.
     */
    public function run(string $seeder): array
    {
        $container = $this->app->boot()->container();
        $instance = $container->get($seeder);
        if (!$instance instanceof Seeder) {
            throw new \InvalidArgumentException("{$seeder} is not a Nimbus\\Database\\Seeder.");
        }
        $manager = $container->has(ArcanoManager::class) ? $container->get(ArcanoManager::class) : null;
        $previous = Factory::useManager($manager instanceof ArcanoManager ? $manager : null);
        try {
            return $instance->invoke($container);
        } finally {
            Factory::useManager($previous);
        }
    }
}
