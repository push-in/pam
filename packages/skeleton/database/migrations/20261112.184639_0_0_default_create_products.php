<?php

declare(strict_types=1);

namespace Migration;

use Arcano\Migrations\Migration;

class OrmDefault930e33fb93139461c4b8315e55f307c4 extends Migration
{
    protected const DATABASE = 'default';

    public function up(): void
    {
        $this->table('products')
        ->addColumn('id', 'primary', ['nullable' => false, 'defaultValue' => null])
        ->addColumn('created_at', 'datetime', ['nullable' => false, 'defaultValue' => null])
        ->addColumn('updated_at', 'datetime', ['nullable' => false, 'defaultValue' => null])
        ->addColumn('name', 'string', ['nullable' => false, 'defaultValue' => null, 'size' => 255])
        ->addColumn('price_in_cents', 'integer', ['nullable' => false, 'defaultValue' => null])
        ->addColumn('status', 'tinyInteger', ['nullable' => false, 'defaultValue' => 1])
        ->setPrimaryKeys(['id'])
        ->create();
    }

    public function down(): void
    {
        $this->table('products')->drop();
    }
}
