<?php

declare(strict_types=1);

namespace Pam\Api;

enum RouteConstraint: int
{
    case Integer = 1;
    case Uuid = 2;
    case Ulid = 3;
    case Slug = 4;
    case Alpha = 5;
    case AlphaNumeric = 6;

    public function pattern(): string
    {
        return match ($this) {
            self::Integer => '[0-9]+',
            self::Uuid => '[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[1-8][0-9a-fA-F]{3}-[89abAB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}',
            self::Ulid => '[0-7][0-9A-HJKMNP-TV-Z]{25}',
            self::Slug => '[A-Za-z0-9]+(?:-[A-Za-z0-9]+)*',
            self::Alpha => '[A-Za-z]+',
            self::AlphaNumeric => '[A-Za-z0-9]+',
        };
    }
}

