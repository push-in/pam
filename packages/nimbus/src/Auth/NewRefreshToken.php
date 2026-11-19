<?php

declare(strict_types=1);

namespace Nimbus\Auth;

/** A refresh token just issued: `plainTextToken` goes to the client once and is never stored. */
final readonly class NewRefreshToken
{
    public function __construct(
        #[\SensitiveParameter]
        public string $plainTextToken,
        public RefreshToken $token,
    ) {
    }
}
