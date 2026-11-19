<?php

declare(strict_types=1);

namespace Nimbus\Validation;

final class Rule
{
    /** @param class-string<\BackedEnum> $enum */
    public static function enum(string $enum): EnumRule
    {
        return new EnumRule($enum);
    }
}

