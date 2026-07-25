<?php

declare(strict_types=1);

namespace Pam\Laravel\Support;

final class ConfigValue
{
    public static function string(string $key, string $default = ''): string
    {
        $value = config($key, $default);

        return is_string($value) ? $value : $default;
    }

    public static function int(string $key, int $default = 0): int
    {
        $value = config($key, $default);

        return is_int($value) ? $value : (is_numeric($value) ? (int) $value : $default);
    }

    public static function float(string $key, float $default = 0.0): float
    {
        $value = config($key, $default);

        return is_float($value) || is_int($value) ? (float) $value : (is_numeric($value) ? (float) $value : $default);
    }

    public static function bool(string $key, bool $default = false): bool
    {
        $value = config($key, $default);
        if (is_bool($value)) {
            return $value;
        }
        if (is_int($value)) {
            return $value !== 0;
        }
        if (is_string($value)) {
            $filtered = filter_var($value, FILTER_VALIDATE_BOOL, FILTER_NULL_ON_FAILURE);
            return $filtered ?? $default;
        }

        return $default;
    }

    /** @return list<string> */
    public static function stringList(string $key): array
    {
        $value = config($key, []);
        if (!is_array($value)) {
            return [];
        }

        return array_values(array_filter($value, static fn (mixed $item): bool => is_string($item)));
    }

    /** @return array<string, bool|float|int|string> */
    public static function scalarMap(string $key): array
    {
        $value = config($key, []);
        if (!is_array($value)) {
            return [];
        }
        $result = [];
        foreach ($value as $name => $item) {
            if (is_string($name) && (is_bool($item) || is_float($item) || is_int($item) || is_string($item))) {
                $result[$name] = $item;
            }
        }

        return $result;
    }
}
