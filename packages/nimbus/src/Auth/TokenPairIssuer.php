<?php

declare(strict_types=1);

namespace Nimbus\Auth;

use Nimbus\Http\HttpException;
use Nimbus\Http\ProblemCode;

/**
 * Issues access tokens (HMAC or JWT) together with rotating refresh tokens, and renews them.
 */
final readonly class TokenPairIssuer implements LoginTokenIssuer
{
    public function __construct(
        private AccessTokenIssuer $accessTokens,
        private RefreshTokens $refreshTokens,
        private int $accessTtlSeconds = 900,
    ) {
        if ($accessTtlSeconds < 1 || $accessTtlSeconds > 86_400) {
            throw new \InvalidArgumentException('Access token TTL must be between one second and one day.');
        }
    }

    /** @param list<string> $abilities */
    public function issue(string $subject, array $abilities = ['*']): TokenPair
    {
        $abilities = Abilities::normalize($abilities);
        return $this->pair($this->refreshTokens->issue($subject, $abilities));
    }

    /**
     * Rotates the refresh token and issues a new access token with the family's abilities.
     *
     * @throws HttpException 401 when the token is invalid, expired, revoked or reused.
     */
    public function refresh(#[\SensitiveParameter] string $refreshToken): TokenPair
    {
        try {
            $next = $this->refreshTokens->rotate($refreshToken);
        } catch (RefreshTokenReusedException $reused) {
            throw new HttpException(401, ProblemCode::Unauthenticated, 'The refresh token is invalid.', [], $reused);
        }
        if ($next === null) {
            throw new HttpException(401, ProblemCode::Unauthenticated, 'The refresh token is invalid.');
        }
        return $this->pair($next);
    }

    /** Logs the refresh token's family out. */
    public function revoke(#[\SensitiveParameter] string $refreshToken): bool
    {
        return $this->refreshTokens->revoke($refreshToken);
    }

    public function issueLoginToken(string $subject, array $abilities = ['*']): array
    {
        return $this->issue($subject, $abilities)->toArray();
    }

    private function pair(NewRefreshToken $refresh): TokenPair
    {
        $token = $refresh->token;
        return new TokenPair(
            $this->accessTokens->issue($token->subject, $token->abilities, $this->accessTtlSeconds),
            $this->accessTtlSeconds,
            $refresh->plainTextToken,
            $token->expiresAt,
            $token->abilities,
        );
    }
}
