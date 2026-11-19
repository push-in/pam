<?php

declare(strict_types=1);

namespace Database\Factories;

use App\Models\User;
use Nimbus\Database\Factory;
use Nimbus\Security\Hash;

/** @extends Factory<User> */
final class UserFactory extends Factory
{
    protected string $model = User::class;

    private static ?string $password = null;

    public function definition(): array
    {
        return [
            'name' => $this->faker->name(),
            'email' => $this->faker->unique()->safeEmail(),
            // Hashing is slow on purpose; every factory user shares one hash of "password".
            'password' => self::$password ??= Hash::make('password'),
            'createdAt' => new \DateTimeImmutable(),
        ];
    }
}
