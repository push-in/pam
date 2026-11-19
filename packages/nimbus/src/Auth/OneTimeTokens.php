<?php

declare(strict_types=1);

namespace Nimbus\Auth;

use Arcano\Database\DatabaseInterface;
use Nimbus\Database\ArcanoDatabaseProvider;

/**
 * Expiring, single-use tokens for a purpose (password reset, e-mail verification) in the
 * `auth_tokens` table. Only the SHA-256 of each 256-bit secret is stored; a new token
 * replaces the subject's previous ones for the same purpose, and consuming one is an atomic
 * compare-and-swap, so it works once even under concurrent requests.
 */
final readonly class OneTimeTokens
{
    private const string TABLE = 'auth_tokens';

    private AuthDatabase $database;

    /** @param (\Closure(): int)|null $clock */
    public function __construct(
        ArcanoDatabaseProvider|DatabaseInterface $database,
        ?string $connection = null,
        ?\Closure $clock = null,
    ) {
        $this->database = new AuthDatabase($database, $connection, $clock);
    }

    /** Creates a token and returns its plain text, which only the recipient should ever see. */
    public function create(string $purpose, string $subject, int $ttlSeconds): string
    {
        self::assertPurpose($purpose);
        if ($subject === '' || strlen($subject) > 191 || $ttlSeconds < 1 || $ttlSeconds > 604_800) {
            throw new \InvalidArgumentException('One-time token subject and TTL (up to seven days) are invalid.');
        }
        $id = AuthDatabase::identifier();
        $secret = AuthDatabase::secret();
        $now = $this->database->now();
        $db = $this->database->db();
        $db->transaction(static function () use ($db, $id, $secret, $purpose, $subject, $now, $ttlSeconds): void {
            $db->delete(self::TABLE, ['purpose' => $purpose, 'subject' => $subject])->run();
            $db->insert(self::TABLE)->values([
                'id' => $id,
                'purpose' => $purpose,
                'subject' => $subject,
                'token_hash' => AuthDatabase::hash($secret),
                'expires_at' => $now + $ttlSeconds,
                'created_at' => $now,
            ])->run();
        });
        return "{$id}.{$secret}";
    }

    /** Spends the token and returns its subject; null when it is unknown, expired, spent or for another purpose. */
    public function consume(string $purpose, #[\SensitiveParameter] string $token): ?string
    {
        $parsed = AuthDatabase::parse($token);
        if ($parsed === null) {
            return null;
        }
        $row = $this->database->first(
            self::TABLE,
            ['id', 'subject', 'token_hash', 'expires_at'],
            ['id' => $parsed[0], 'purpose' => $purpose],
        );
        if ($row === null || !hash_equals(AuthDatabase::string($row['token_hash'] ?? ''), AuthDatabase::hash($parsed[1]))) {
            return null;
        }
        $deleted = $this->database->db()->delete(self::TABLE, ['id' => $parsed[0], 'purpose' => $purpose])->run();
        if ($deleted !== 1 || AuthDatabase::int($row['expires_at'] ?? 0) <= $this->database->now()) {
            return null;
        }
        return AuthDatabase::string($row['subject'] ?? '');
    }

    /** Whether the subject got a token for the purpose in the last `$seconds`, for throttling. */
    public function createdWithin(string $purpose, string $subject, int $seconds): bool
    {
        $row = $this->database->first(self::TABLE, ['created_at'], ['purpose' => $purpose, 'subject' => $subject]);
        return $row !== null && $this->database->now() - AuthDatabase::int($row['created_at'] ?? 0) < $seconds;
    }

    public function invalidate(string $purpose, string $subject): int
    {
        return $this->database->db()->delete(self::TABLE, ['purpose' => $purpose, 'subject' => $subject])->run();
    }

    /** Deletes expired tokens. */
    public function prune(): int
    {
        return $this->database->db()->delete(self::TABLE)
            ->where('expires_at', '<=', $this->database->now())
            ->run();
    }

    private static function assertPurpose(string $purpose): void
    {
        if (preg_match('/^[a-z0-9][a-z0-9._-]{0,63}$/D', $purpose) !== 1) {
            throw new \InvalidArgumentException('One-time token purposes are short lowercase identifiers.');
        }
    }
}
