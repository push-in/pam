<?php

declare(strict_types=1);

namespace Nimbus\Auth\OAuth;

/**
 * Server-side storage of pending OAuth authorizations (state and PKCE verifier), shared by
 * every worker that may receive the callback. Each state is read at most once.
 */
interface OAuthStateStore
{
    /** @param array<string, string> $data */
    public function put(string $state, array $data, int $expiresAt): void;

    /**
     * Returns and forgets the state's data; null when unknown, already used or expired.
     *
     * @return array<string, string>|null
     */
    public function pull(string $state, int $now): ?array;
}
