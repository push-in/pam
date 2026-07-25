<?php

declare(strict_types=1);

namespace Pam\Laravel\Enums;

enum SpanStatus: int
{
    case Unset = 1;
    case Ok = 2;
    case Error = 3;

    public function otlpCode(): int
    {
        return match ($this) {
            self::Unset => 0,
            self::Ok => 1,
            self::Error => 2,
        };
    }
}
