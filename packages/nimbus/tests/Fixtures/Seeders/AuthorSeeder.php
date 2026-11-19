<?php

declare(strict_types=1);

namespace Nimbus\Tests\Fixtures\Seeders;

use Nimbus\Database\Seeder;
use Nimbus\Tests\Fixtures\Factories\AuthorFactory;

final class AuthorSeeder extends Seeder
{
    public function run(): void
    {
        AuthorFactory::new()->count(2)->sequence(['name' => 'Ada'], ['name' => 'Grace'])->create();
    }
}
