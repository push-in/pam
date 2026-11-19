<?php

declare(strict_types=1);

namespace Nimbus\Tests\Fixtures;

final readonly class LoginService
{
    public function message(string $email): string
    {
        return "authenticated:{$email}";
    }
}

