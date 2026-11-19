<?php

declare(strict_types=1);

namespace Database\Factories;

use App\Models\Product;
use App\Models\ProductStatus;
use Nimbus\Database\Factory;

/** @extends Factory<Product> */
final class ProductFactory extends Factory
{
    protected string $model = Product::class;

    public function definition(): array
    {
        return [
            'name' => ucfirst($this->faker->words(2, true)),
            'priceInCents' => $this->faker->numberBetween(100, 100_000),
        ];
    }

    public function archived(): self
    {
        return $this->state(['status' => ProductStatus::Archived]);
    }
}
