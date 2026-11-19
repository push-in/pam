<?php

declare(strict_types=1);

namespace Nimbus\Security;

/** Password hashing with Argon2id where available, bcrypt otherwise. */
final readonly class Hash
{
    public static function make(#[\SensitiveParameter] string $password): string
    {
        return password_hash($password, self::algorithm());
    }

    public static function check(#[\SensitiveParameter] string $password, string $hash): bool
    {
        return password_verify($password, $hash);
    }

    /** True when the hash was made with an older algorithm or cost; rehash after a successful check. */
    public static function needsRehash(string $hash): bool
    {
        return password_needs_rehash($hash, self::algorithm());
    }

    private static function algorithm(): string
    {
        return defined('PASSWORD_ARGON2ID') ? PASSWORD_ARGON2ID : PASSWORD_BCRYPT;
    }
}
