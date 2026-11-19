<?php

declare(strict_types=1);

namespace Nimbus\Auth;

use Arcano\Database\DatabaseInterface;
use Nimbus\Database\ArcanoDatabaseProvider;

/**
 * Revoked access-token identifiers (`jti`) in the `revoked_tokens` table, shared by every
 * worker and server. Rows are kept until the token would have expired; `prune()` drops them.
 */
final readonly class DatabaseTokenRevocationStore implements TokenRevocationStore
{
    private const string TABLE = 'revoked_tokens';

    private AuthDatabase $database;

    /** @param (\Closure(): int)|null $clock */
    public function __construct(
        ArcanoDatabaseProvider|DatabaseInterface $database,
        ?string $connection = null,
        ?\Closure $clock = null,
    ) {
        $this->database = new AuthDatabase($database, $connection, $clock);
    }

    public function revoke(string $tokenIdentifier, int $expiresAt): void
    {
        if ($tokenIdentifier === '' || strlen($tokenIdentifier) > 191 || $expiresAt < 1) {
            throw new \InvalidArgumentException('Revoked token identifier and expiry are required.');
        }
        $db = $this->database->db();
        $db->transaction(static function () use ($db, $tokenIdentifier, $expiresAt): void {
            $db->delete(self::TABLE, ['id' => $tokenIdentifier])->run();
            $db->insert(self::TABLE)->values(['id' => $tokenIdentifier, 'expires_at' => $expiresAt])->run();
        });
    }

    public function isRevoked(string $tokenIdentifier, int $now): bool
    {
        return $this->database->db()->select()->from(self::TABLE)
            ->where('id', $tokenIdentifier)
            ->where('expires_at', '>', $now)
            ->count() > 0;
    }

    /** Deletes entries of tokens that have expired anyway. */
    public function prune(): int
    {
        return $this->database->db()->delete(self::TABLE)
            ->where('expires_at', '<=', $this->database->now())
            ->run();
    }
}
