<?php

declare(strict_types=1);

namespace Nimbus\Tests\Jobs;

use Nimbus\Database\ArcanoDatabaseProvider;
use Nimbus\Database\ArcanoMigrationManager;
use Nimbus\Jobs\QueueTablesMigration;
use Nimbus\Tests\Database\ArcanoTestDatabase;

/** A migrated database with the queue tables, removed by drop(). */
final class QueueDatabase
{
    public readonly ArcanoDatabaseProvider $databases;

    private readonly ArcanoMigrationManager $migrations;

    private function __construct(private readonly string $directory)
    {
        mkdir($directory . '/migrations', 0o777, true);
        $this->databases = new ArcanoDatabaseProvider(ArcanoTestDatabase::config($directory . '/queue.sqlite'));
        $this->migrations = new ArcanoMigrationManager($this->databases, $directory . '/migrations');
    }

    public static function create(): self
    {
        $database = new self(sys_get_temp_dir() . '/nimbus-queue-' . bin2hex(random_bytes(6)));
        $published = $database->migrations->publish('create_queue_tables', QueueTablesMigration::class);
        if ($published === null || $database->migrations->publish('create_queue_tables', QueueTablesMigration::class) !== null) {
            throw new \LogicException('The queue migration must publish exactly once.');
        }
        $database->migrations->migrate();
        return $database;
    }

    public function drop(): void
    {
        $this->migrations->rollback();
        $this->databases->releaseCurrent();
        $this->databases->disconnectIdle();
        foreach (glob($this->directory . '/migrations/*') ?: [] as $file) {
            unlink($file);
        }
        @rmdir($this->directory . '/migrations');
        @unlink($this->directory . '/queue.sqlite');
        @rmdir($this->directory);
    }
}
