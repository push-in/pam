<?php

declare(strict_types=1);

namespace Pam\Laravel\Enums;

enum CheckStatus: int
{
    case Pass = 1;
    case Warning = 2;
    case Failure = 3;

    public function label(): string
    {
        return match ($this) {
            self::Pass => 'PASS',
            self::Warning => 'WARN',
            self::Failure => 'FAIL',
        };
    }
}
