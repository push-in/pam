<?php

declare(strict_types=1);

namespace Pam\Api\Database;

use Arcano\Migrations\Config\MigrationConfig;
use Arcano\Migrations\FileRepository;
use Arcano\Migrations\MigrationInterface;
use Arcano\Migrations\Migrator;
use Arcano\Migrations\State;
use Arcano\Schema\Generator\Migrations\GenerateMigrations;

/**
 * Arcano migrations of one application directory.
 */
final class ArcanoMigrationManager
{
    private readonly MigrationConfig $config;

    public function __construct(
        private readonly ArcanoDatabaseProvider $databases,
        string $directory,
        string $table = 'migrations',
        string $namespace = 'Migration',
        private readonly ?ArcanoSchemaCompiler $entities = null,
    ) {
        $this->config = new MigrationConfig([
            'directory' => rtrim($directory, '/') . '/',
            'table' => $table,
            'namespace' => $namespace,
            'safe' => true,
        ]);
    }

    /**
     * Runs every pending migration, each in its own transaction.
     *
     * @return list<string> Migration names in execution order.
     */
    public function migrate(): array
    {
        $migrator = $this->migrator();
        $migrator->configure();
        $completed = [];
        while (($migration = $migrator->run()) instanceof MigrationInterface) {
            $completed[] = $migration->getState()->getName();
        }
        return $completed;
    }

    /**
     * Reverts the last executed migrations.
     *
     * @param positive-int $steps
     * @return list<string> Migration names in rollback order.
     */
    public function rollback(int $steps = 1): array
    {
        $migrator = $this->migrator();
        if (!$migrator->isConfigured()) {
            return [];
        }
        $reverted = [];
        for ($step = 0; $step < $steps; $step++) {
            $migration = $migrator->rollback();
            if (!$migration instanceof MigrationInterface) {
                break;
            }
            $reverted[] = $migration->getState()->getName();
        }
        return $reverted;
    }

    /**
     * @return list<array{name: string, created_at: \DateTimeInterface, executed_at: ?\DateTimeInterface}>
     */
    public function status(): array
    {
        $migrator = $this->migrator();
        $migrator->configure();
        $status = [];
        foreach ($migrator->getMigrations() as $migration) {
            $state = $migration->getState();
            $status[] = [
                'name' => $state->getName(),
                'created_at' => $state->getTimeCreated(),
                'executed_at' => $state->getStatus() === State::STATUS_EXECUTED ? $state->getTimeExecuted() : null,
            ];
        }
        return $status;
    }

    /**
     * Writes a migration with the differences between the entities and the database.
     * Pending migrations must run first, so the database reflects every existing file.
     *
     * @return list<string> Paths of the new migration files; empty when nothing changed.
     */
    public function generate(): array
    {
        if ($this->entities === null) {
            throw new \LogicException('Generating migrations requires the ArcanoSchemaCompiler of the entities.');
        }
        $migrator = $this->migrator();
        $migrator->configure();
        foreach ($migrator->getMigrations() as $migration) {
            if ($migration->getState()->getStatus() === State::STATUS_PENDING) {
                throw new \LogicException(
                    "Run the pending migration {$migration->getState()->getName()} before generating new ones.",
                );
            }
        }

        $directory = $this->config->getDirectory();
        $before = self::files($directory);
        $this->entities->compile($this->databases->manager(), [
            new GenerateMigrations($migrator->getRepository(), $this->config),
        ]);
        return array_values(array_diff(self::files($directory), $before));
    }

    /** Bound to the connections of the current Fiber. */
    private function migrator(): Migrator
    {
        if (!is_dir($this->config->getDirectory())) {
            throw new \InvalidArgumentException("Migration directory {$this->config->getDirectory()} does not exist.");
        }
        return new Migrator(
            $this->config,
            $this->databases->manager(),
            new FileRepository($this->config),
        );
    }

    /** @return list<string> */
    private static function files(string $directory): array
    {
        $files = glob($directory . '*.php');
        return $files === false ? [] : $files;
    }
}
