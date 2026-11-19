<?php

declare(strict_types=1);

namespace Nimbus\Auth;

/** A stored refresh token of a rotation family. */
final readonly class RefreshToken
{
    /** @param list<string> $abilities */
    public function __construct(
        public string $id,
        public string $familyId,
        public string $subject,
        public array $abilities,
        public int $expiresAt,
    ) {
    }
}
