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

    /** @param list<string|int|float|bool|\BackedEnum> $values */
    public static function in(array $values): InRule
    {
        return new InRule($values);
    }

    /** @param list<string|int|float|bool|\BackedEnum> $values */
    public static function notIn(array $values): InRule
    {
        return new InRule($values, true);
    }

    /** The value exists in `$table.$column` (`connection.table` picks the connection). */
    public static function exists(string $table, string $column): ExistsRule
    {
        return new ExistsRule($table, $column);
    }

    /** No row of `$table` has the value in `$column`; chain `ignore($id)` on updates. */
    public static function unique(string $table, string $column): UniqueRule
    {
        return new UniqueRule($table, $column);
    }
}
