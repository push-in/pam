<?php

declare(strict_types=1);

namespace Migration;

use Arcano\Migrations\Migration;

class NimbusTestingCreateAuthorsPosts extends Migration
{
    protected const DATABASE = 'default';

    public function up(): void
    {
        $this->table('authors')
            ->addColumn('id', 'primary', ['nullable' => false, 'defaultValue' => null])
            ->addColumn('name', 'string', ['nullable' => false, 'defaultValue' => null, 'size' => 120])
            ->setPrimaryKeys(['id'])
            ->create();
        $this->table('posts')
            ->addColumn('id', 'primary', ['nullable' => false, 'defaultValue' => null])
            ->addColumn('title', 'string', ['nullable' => false, 'defaultValue' => null, 'size' => 255])
            ->addColumn('published', 'boolean', ['nullable' => false, 'defaultValue' => false])
            ->addColumn('author_id', 'integer', ['nullable' => false, 'defaultValue' => null])
            ->setPrimaryKeys(['id'])
            ->create();
    }

    public function down(): void
    {
        $this->table('posts')->drop();
        $this->table('authors')->drop();
    }
}
