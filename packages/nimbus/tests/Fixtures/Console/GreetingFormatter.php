<?php

declare(strict_types=1);

namespace Nimbus\Tests\Fixtures\Console;

final class GreetingFormatter
{
    public function format(string $prefix, string $name): string
    {
        return "{$prefix}, {$name}!";
    }
}
