<?php

declare(strict_types=1);

namespace Pam\Api\Auth;

final class MemoryTokenRevocationStore implements TokenRevocationStore
{
    /** @var array<string, int> */
    private array $revoked = [];

    public function __construct(private readonly int $maximumEntries = 10_000)
    {
        if ($maximumEntries < 1 || $maximumEntries > 1_000_000) {
            throw new \InvalidArgumentException('Revocation capacity must be between 1 and 1,000,000 entries.');
        }
    }

    public function revoke(string $tokenIdentifier, int $expiresAt): void
    {
        if ($tokenIdentifier === '' || $expiresAt < 1) {
            throw new \InvalidArgumentException('Revoked token identifier and expiry are required.');
        }
        $this->revoked[$tokenIdentifier] = $expiresAt;
        if (count($this->revoked) <= $this->maximumEntries) {
            return;
        }
        asort($this->revoked, SORT_NUMERIC);
        array_shift($this->revoked);
    }

    public function isRevoked(string $tokenIdentifier, int $now): bool
    {
        foreach ($this->revoked as $identifier => $expiresAt) {
            if ($expiresAt <= $now) {
                unset($this->revoked[$identifier]);
            }
        }
        return isset($this->revoked[$tokenIdentifier]);
    }
}
