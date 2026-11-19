<?php

declare(strict_types=1);

namespace Nimbus\Tests\Fixtures\Factories;

use Nimbus\Database\Factory;
use Nimbus\Tests\Fixtures\Arcano\Post;

/** @extends Factory<Post> */
final class PostFactory extends Factory
{
    protected string $model = Post::class;

    public function definition(): array
    {
        return [
            'title' => $this->faker->sentence(3),
            'published' => false,
            'author' => AuthorFactory::new(),
        ];
    }

    public function published(): self
    {
        return $this->state(['published' => true]);
    }
}
