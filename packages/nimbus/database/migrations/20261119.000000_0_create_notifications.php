<?php

declare(strict_types=1);

namespace Migration;

use Arcano\Migrations\Migration;

/**
 * Table of Nimbus\Notifications\Database\DatabaseChannel. Copy it into the application's
 * migrations directory (or point ArcanoMigrationManager at vendor/pushinbr/nimbus/database/migrations).
 */
class CreateNimbusNotifications extends Migration
{
    protected const DATABASE = 'default';

    public function up(): void
    {
        $this->table('notifications')
        ->addColumn('id', 'string', ['nullable' => false, 'defaultValue' => null, 'size' => 36])
        ->addColumn('type', 'string', ['nullable' => false, 'defaultValue' => null, 'size' => 255])
        ->addColumn('notifiable_type', 'string', ['nullable' => false, 'defaultValue' => null, 'size' => 191])
        ->addColumn('notifiable_id', 'string', ['nullable' => false, 'defaultValue' => null, 'size' => 64])
        ->addColumn('data', 'text', ['nullable' => false, 'defaultValue' => null])
        ->addColumn('read_at', 'datetime', ['nullable' => true, 'defaultValue' => null])
        ->addColumn('created_at', 'datetime', ['nullable' => false, 'defaultValue' => null])
        ->setPrimaryKeys(['id'])
        ->addIndex(['notifiable_type', 'notifiable_id', 'created_at'], ['name' => 'notifications_notifiable_index'])
        ->create();
    }

    public function down(): void
    {
        $this->table('notifications')->drop();
    }
}
