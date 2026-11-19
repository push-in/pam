<?php

declare(strict_types=1);

namespace Nimbus\Tests\Fixtures\Factories;

use Nimbus\Database\Factory;
use Nimbus\Tests\Fixtures\Arcano\Author;

/** @extends Factory<Author> */
final class AuthorFactory extends Factory
{
    protected string $model = Author::class;

    public function definition(): array
    {
        return ['name' => $this->faker->name()];
    }
}
