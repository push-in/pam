<?php

declare(strict_types=1);

namespace Nimbus\Tests\Fixtures\Seeders;

use Nimbus\Database\Seeder;

final class DatabaseSeeder extends Seeder
{
    public function run(): void
    {
        $this->call([AuthorSeeder::class, PostSeeder::class]);
    }
}
