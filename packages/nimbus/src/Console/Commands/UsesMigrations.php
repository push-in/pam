<?php

declare(strict_types=1);

namespace Nimbus\Console\Commands;

use Nimbus\Database\ArcanoMigrationManager;

/** @internal */
trait UsesMigrations
{
    private function migrations(): ArcanoMigrationManager
    {
        $container = $this->app()->container();
        $migrations = $container->has(ArcanoMigrationManager::class) ? $container->get(ArcanoMigrationManager::class) : null;
        return $migrations instanceof ArcanoMigrationManager ? $migrations : throw new \RuntimeException(
            'No migrations: pass migrations: to ArcanoServiceProvider in bootstrap/app.php.',
        );
    }
}
