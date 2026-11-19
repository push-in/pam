<?php

declare(strict_types=1);

namespace Nimbus\Tests\Database;

use Nimbus\Database\Factory;
use Nimbus\Tests\Fixtures\Arcano\Author;
use Nimbus\Tests\Fixtures\Arcano\Post;
use Nimbus\Tests\Fixtures\Factories\AuthorFactory;
use Nimbus\Tests\Fixtures\Factories\PostFactory;
use Nimbus\Tests\Testing\ArcanoTestCase;
use PHPUnit\Framework\Attributes\CoversClass;

#[CoversClass(Factory::class)]
final class FactoryTest extends ArcanoTestCase
{
    public function testMakeBuildsUnsavedEntitiesWithFakeData(): void
    {
        $author = AuthorFactory::new()->makeOne();
        self::assertNull($author->id);
        self::assertNotSame('', $author->name);

        $authors = AuthorFactory::new()->count(3)->make();
        self::assertIsArray($authors);
        self::assertCount(3, $authors);
        self::assertInstanceOf(Author::class, AuthorFactory::new()->make(['name' => 'Ada']));
        $this->assertDatabaseCount('authors', 0);
    }

    public function testStatesSequencesAndOverridesApplyInOrder(): void
    {
        $posts = PostFactory::new()
            ->count(3)
            ->published()
            ->state(static fn (array $attributes): array => ['title' => 'Draft of ' . (is_string($attributes['title']) ? 'x' : '?')])
            ->sequence(['title' => 'First'], static fn (int $index): array => ['title' => "Post {$index}"])
            ->makeMany();

        self::assertSame(['First', 'Post 1', 'First'], array_map(static fn (Post $post): string => $post->title, $posts));
        self::assertTrue($posts[0]->published);
        self::assertInstanceOf(Author::class, $posts[0]->author);
        self::assertNotSame($posts[0]->author, $posts[1]->author);

        $post = PostFactory::new()->makeOne([
            'title' => 'Fixed',
            'published' => static fn (array $attributes): bool => $attributes['title'] === 'Fixed',
        ]);
        self::assertSame('Fixed', $post->title);
        self::assertTrue($post->published);
    }

    public function testRawReturnsAttributes(): void
    {
        $raw = AuthorFactory::new()->raw(['name' => 'Ada']);
        self::assertSame(['name' => 'Ada'], $raw);
        self::assertCount(2, AuthorFactory::new()->count(2)->raw());
        self::assertIsArray(PostFactory::new()->raw()['author'] ?? null);
    }

    public function testCreatePersistsEntitiesAndTheirRelations(): void
    {
        $made = [];
        $created = [];
        $posts = PostFactory::new()
            ->count(2)
            ->afterMaking(static function (Post $post) use (&$made): void {
                $made[] = $post->title;
            })
            ->afterCreating(static function (Post $post) use (&$created): void {
                $created[] = $post->id;
            })
            ->create(['author' => AuthorFactory::new()->createOne(['name' => 'Ada'])]);

        self::assertIsArray($posts);
        self::assertCount(2, $made);
        self::assertCount(2, $created);
        self::assertNotNull($created[0]);
        $this->assertDatabaseCount(Post::class, 2);
        $this->assertDatabaseCount('authors', 1);
        $this->assertDatabaseHas('posts', ['id' => $created[1], 'published' => false]);

        PostFactory::new()->createOne();
        $this->assertDatabaseCount('authors', 2);
        $this->getJson('/posts')->assertOk()->assertJsonCount(3, 'data')->assertJsonPath('data.0.author', 'Ada');
    }

    public function testCreateNeedsAManager(): void
    {
        Factory::useManager(null);
        $this->expectException(\LogicException::class);
        AuthorFactory::new()->createOne();
    }
}
