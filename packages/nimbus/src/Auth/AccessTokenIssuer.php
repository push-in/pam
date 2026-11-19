<?php

declare(strict_types=1);

namespace Nimbus\Auth;

/** Issues short-lived signed access tokens (`HmacTokenCodec`, `JwtCodec`). */
interface AccessTokenIssuer
{
    /** @param list<string> $abilities */
    public function issue(string $subject, array $abilities, int $ttlSeconds = 900): string;
}
