<?php

declare(strict_types=1);

namespace Nimbus\Auth;

/** A short-lived access token with the refresh token that renews it. */
final readonly class TokenPair
{
    /** @param list<string> $abilities */
    public function __construct(
        #[\SensitiveParameter]
        public string $accessToken,
        public int $expiresIn,
        #[\SensitiveParameter]
        public string $refreshToken,
        public int $refreshExpiresAt,
        public array $abilities,
    ) {
    }

    /**
     * The OAuth 2 token response shape.
     *
     * @return array{token_type: string, access_token: string, expires_in: int, refresh_token: string, refresh_expires_at: int, scope: string}
     */
    public function toArray(): array
    {
        return [
            'token_type' => 'Bearer',
            'access_token' => $this->accessToken,
            'expires_in' => $this->expiresIn,
            'refresh_token' => $this->refreshToken,
            'refresh_expires_at' => $this->refreshExpiresAt,
            'scope' => implode(' ', $this->abilities),
        ];
    }
}
