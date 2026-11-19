<?php

declare(strict_types=1);

namespace Nimbus\Auth\OAuth;

use Arcano\Database\DatabaseInterface;
use Nimbus\Auth\AuthDatabase;
use Nimbus\Database\ArcanoDatabaseProvider;

/** Pending OAuth authorizations in the `oauth_states` table, keyed by the SHA-256 of the state. */
final readonly class DatabaseOAuthStateStore implements OAuthStateStore
{
    private const string TABLE = 'oauth_states';

    private AuthDatabase $database;

    /** @param (\Closure(): int)|null $clock */
    public function __construct(
        ArcanoDatabaseProvider|DatabaseInterface $database,
        ?string $connection = null,
        ?\Closure $clock = null,
    ) {
        $this->database = new AuthDatabase($database, $connection, $clock);
    }

    public function put(string $state, array $data, int $expiresAt): void
    {
        $this->database->db()->insert(self::TABLE)->values([
            'id' => OAuthStateCodec::key($state),
            'payload' => OAuthStateCodec::encode($data, $expiresAt),
            'expires_at' => $expiresAt,
        ])->run();
    }

    public function pull(string $state, int $now): ?array
    {
        $key = OAuthStateCodec::key($state);
        $row = $this->database->first(self::TABLE, ['payload'], ['id' => $key]);
        if ($row === null || $this->database->db()->delete(self::TABLE, ['id' => $key])->run() !== 1) {
            return null;
        }
        return OAuthStateCodec::decode($row['payload'] ?? null, $now);
    }

    /** Deletes abandoned authorizations. */
    public function prune(): int
    {
        return $this->database->db()->delete(self::TABLE)
            ->where('expires_at', '<=', $this->database->now())
            ->run();
    }
}
