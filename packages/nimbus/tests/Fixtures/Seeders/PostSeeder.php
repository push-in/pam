<?php

declare(strict_types=1);

namespace Nimbus\Tests\Fixtures\Seeders;

use Nimbus\Database\ArcanoManager;
use Nimbus\Database\Seeder;
use Nimbus\Tests\Fixtures\Arcano\Author;
use Nimbus\Tests\Fixtures\Factories\PostFactory;

/** Asks the container for the manager, like application seeders do. */
final class PostSeeder extends Seeder
{
    public function run(ArcanoManager $arcano): void
    {
        $ada = $arcano->repository(Author::class)->findOne(['name' => 'Ada']);
        if (!$ada instanceof Author) {
            throw new \LogicException('AuthorSeeder must run first.');
        }
        PostFactory::new()->count(3)->createMany(['author' => $ada]);
    }
}
