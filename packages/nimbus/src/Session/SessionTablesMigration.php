<?php

declare(strict_types=1);

namespace Nimbus\Session;

use Arcano\Migrations\Migration;

/**
 * Creates the `sessions` table of DatabaseSessionStore. Publish it into the application's
 * migrations with ArcanoMigrationManager::publish('create_sessions_table', SessionTablesMigration::class).
 */
final class SessionTablesMigration extends Migration
{
    public function up(): void
    {
        $this->table('sessions')
            ->addColumn('id', 'string', ['nullable' => false, 'size' => 40])
            ->addColumn('payload', 'text', ['nullable' => false])
            ->addColumn('expires_at', 'bigInteger', ['nullable' => false])
            ->addColumn('updated_at', 'bigInteger', ['nullable' => false])
            ->addIndex(['expires_at'])
            ->setPrimaryKeys(['id'])
            ->create();
    }

    public function down(): void
    {
        $this->table('sessions')->drop();
    }
}
