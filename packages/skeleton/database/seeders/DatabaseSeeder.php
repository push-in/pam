<?php

declare(strict_types=1);

namespace Database\Seeders;

use Database\Factories\ProductFactory;
use Nimbus\Database\Seeder;

/** Run with `pam composer seed`. */
final class DatabaseSeeder extends Seeder
{
    public function run(): void
    {
        ProductFactory::new()->count(10)->create();
        ProductFactory::new()->count(2)->archived()->create();
    }
}
