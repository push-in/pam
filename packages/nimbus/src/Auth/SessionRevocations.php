<?php

declare(strict_types=1);

namespace Nimbus\Auth;

/**
 * Signs a user's web sessions out on every device: `revokeAll()` records the moment in the
 * `auth_tokens` table, and a `SessionAuthenticator` built with this store rejects sessions
 * that logged in before it. The record lives `$ttlSeconds` (at most seven days, the longest
 * one-time token): keep sessions shorter than that.
 */
final readonly class SessionRevocations implements RevokesCredentials
{
    public const string PURPOSE = 'session-revocation';

    public function __construct(
        private OneTimeTokens $tokens,
        private int $ttlSeconds = 604_800,
    ) {
    }

    public function revokeAll(string $subject): int
    {
        $this->tokens->create(self::PURPOSE, $subject, $this->ttlSeconds);
        return 1;
    }

    /** When the subject's sessions were last revoked, or null. */
    public function revokedAt(string $subject): ?int
    {
        return $this->tokens->createdAt(self::PURPOSE, $subject);
    }
}
