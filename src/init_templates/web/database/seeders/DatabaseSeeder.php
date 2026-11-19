<?php

declare(strict_types=1);

namespace Database\Seeders;

use Database\Factories\UserFactory;
use Nimbus\Database\Seeder;

final class DatabaseSeeder extends Seeder
{
    public function run(): void
    {
        UserFactory::new()->create(['name' => 'Ana Souza', 'email' => 'ana@example.com']);
    }
}
