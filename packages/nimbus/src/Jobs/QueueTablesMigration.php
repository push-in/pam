<?php

declare(strict_types=1);

namespace Nimbus\Jobs;

use Arcano\Migrations\Migration;

/**
 * Creates `jobs` and `failed_jobs` (DatabaseJobQueue) and `scheduler_locks`
 * (DatabaseSchedulerLock). Publish it into the application's migrations with
 * ArcanoMigrationManager::publish('create_queue_tables', QueueTablesMigration::class).
 */
final class QueueTablesMigration extends Migration
{
    public function up(): void
    {
        $this->table('jobs')
            ->addColumn('id', 'string', ['nullable' => false, 'size' => 32])
            ->addColumn('queue', 'string', ['nullable' => false, 'size' => 64])
            ->addColumn('payload', 'text', ['nullable' => false])
            ->addColumn('attempts', 'integer', ['nullable' => false, 'defaultValue' => 0])
            ->addColumn('max_attempts', 'integer', ['nullable' => false, 'defaultValue' => 3])
            ->addColumn('available_at', 'bigInteger', ['nullable' => false])
            ->addColumn('reserved_until', 'bigInteger', ['nullable' => true, 'defaultValue' => null])
            ->addColumn('last_failure', 'string', ['nullable' => true, 'defaultValue' => null, 'size' => 255])
            ->addColumn('created_at', 'bigInteger', ['nullable' => false])
            ->addIndex(['queue', 'available_at'])
            ->addIndex(['queue', 'reserved_until'])
            ->setPrimaryKeys(['id'])
            ->create();

        $this->table('failed_jobs')
            ->addColumn('id', 'string', ['nullable' => false, 'size' => 32])
            ->addColumn('queue', 'string', ['nullable' => false, 'size' => 64])
            ->addColumn('payload', 'text', ['nullable' => false])
            ->addColumn('attempts', 'integer', ['nullable' => false])
            ->addColumn('max_attempts', 'integer', ['nullable' => false])
            ->addColumn('failure_class', 'string', ['nullable' => true, 'defaultValue' => null, 'size' => 255])
            ->addColumn('failed_at', 'bigInteger', ['nullable' => false])
            ->addIndex(['queue', 'failed_at'])
            ->setPrimaryKeys(['id'])
            ->create();

        $this->table('scheduler_locks')
            ->addColumn('name', 'string', ['nullable' => false, 'size' => 191])
            ->addColumn('owner', 'string', ['nullable' => false, 'size' => 32])
            ->addColumn('expires_at', 'bigInteger', ['nullable' => false])
            ->setPrimaryKeys(['name'])
            ->create();
    }

    public function down(): void
    {
        $this->table('scheduler_locks')->drop();
        $this->table('failed_jobs')->drop();
        $this->table('jobs')->drop();
    }
}
