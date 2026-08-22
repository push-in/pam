<?php

declare(strict_types=1);

namespace Pam\Api\Database;

use Illuminate\Database\Migrations\DatabaseMigrationRepository;
use Illuminate\Database\Migrations\Migrator;
use Illuminate\Filesystem\Filesystem;

final readonly class MigrationManager
{
    private Migrator $migrator;

    public function __construct(
        FiberConnectionResolver $connections,
        ?string $connection = null,
        string $table = 'migrations',
    ) {
        $repository = new DatabaseMigrationRepository($connections, $table);
        if ($connection !== null) {
            $repository->setSource($connection);
        }
        $this->migrator = new Migrator(
            $repository,
            $connections,
            new Filesystem(),
            $connections->eventDispatcher(),
        );
        $this->migrator->setConnection($connection);
    }

    /**
     * @param non-empty-list<string>|string $paths
     * @return list<string> Migration names in execution order.
     */
    public function migrate(array|string $paths, bool $pretend = false): array
    {
        $this->ensureRepository();
        return array_map(
            static fn (string $file): string => pathinfo($file, PATHINFO_FILENAME),
            array_values($this->migrator->run($paths, ['pretend' => $pretend])),
        );
    }

    /**
     * @param non-empty-list<string>|string $paths
     * @return list<string> Migration names in rollback order.
     */
    public function rollback(array|string $paths, int $steps = 0, bool $pretend = false): array
    {
        if (!$this->migrator->repositoryExists()) {
            return [];
        }
        return array_map(
            static fn (string $file): string => pathinfo($file, PATHINFO_FILENAME),
            array_values($this->migrator->rollback($paths, [
                'step' => $steps,
                'pretend' => $pretend,
            ])),
        );
    }

    private function ensureRepository(): void
    {
        if (!$this->migrator->repositoryExists()) {
            $this->migrator->getRepository()->createRepository();
        }
    }
}
