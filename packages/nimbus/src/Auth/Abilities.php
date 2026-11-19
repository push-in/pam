<?php

declare(strict_types=1);

namespace Nimbus\Auth;

/**
 * Ability (scope) matching shared by tokens and principals: `*` grants everything and a
 * trailing `:*` or `.*` grants a namespace, so `products:*` allows `products:write`.
 */
final readonly class Abilities
{
    /** @param list<string> $granted */
    public static function allows(array $granted, string $ability): bool
    {
        if ($ability === '') {
            return false;
        }
        foreach ($granted as $candidate) {
            if ($candidate === '*' || $candidate === $ability) {
                return true;
            }
            if (strlen($candidate) > 2 && (str_ends_with($candidate, ':*') || str_ends_with($candidate, '.*'))
                && str_starts_with($ability, substr($candidate, 0, -1))
            ) {
                return true;
            }
        }
        return false;
    }

    /**
     * Validates and deduplicates a list of abilities.
     *
     * @param array<array-key, mixed> $abilities
     * @return list<string>
     */
    public static function normalize(array $abilities): array
    {
        $normalized = [];
        foreach ($abilities as $ability) {
            if (!is_string($ability) || $ability === '' || strlen($ability) > 191
                || preg_match('/^[\x21-\x7E]+$/D', $ability) !== 1
            ) {
                throw new \InvalidArgumentException('Abilities must be non-empty printable strings without spaces.');
            }
            $normalized[$ability] = true;
        }
        return array_keys($normalized);
    }
}
