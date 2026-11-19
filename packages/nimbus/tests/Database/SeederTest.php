<?php

declare(strict_types=1);

namespace Nimbus\Tests\Database;

use Nimbus\Database\Seeder;
use Nimbus\Database\SeederRunner;
use Nimbus\Tests\Fixtures\Arcano\Post;
use Nimbus\Tests\Fixtures\Seeders\AuthorSeeder;
use Nimbus\Tests\Fixtures\Seeders\DatabaseSeeder;
use Nimbus\Tests\Fixtures\Seeders\PostSeeder;
use Nimbus\Tests\Testing\ArcanoTestCase;
use PHPUnit\Framework\Attributes\CoversClass;

#[CoversClass(Seeder::class)]
#[CoversClass(SeederRunner::class)]
final class SeederTest extends ArcanoTestCase
{
    public function testTheRunnerCallsNestedSeedersThroughTheContainer(): void
    {
        $ran = $this->seed(DatabaseSeeder::class);

        self::assertSame([DatabaseSeeder::class, AuthorSeeder::class, PostSeeder::class], $ran);
        $this->assertDatabaseCount('authors', 2);
        $this->assertDatabaseHas('authors', ['name' => 'Grace']);
        $this->assertDatabaseCount(Post::class, 3);
        $this->getJson('/posts')->assertJsonPath('data.2.author', 'Ada');
    }

    public function testSeedersRunWithoutAContainer(): void
    {
        $seeder = new class extends Seeder {
            public int $runs = 0;

            public function run(): void
            {
                ++$this->runs;
            }
        };

        self::assertSame([$seeder::class], $seeder->invoke());
        self::assertSame(1, $seeder->runs);
    }

    public function testTheRunnerRejectsOtherClasses(): void
    {
        $this->expectException(\InvalidArgumentException::class);
        /** @phpstan-ignore argument.type */
        (new SeederRunner($this->application()))->run(\stdClass::class);
    }
}
