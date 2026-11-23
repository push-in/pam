<?php

declare(strict_types=1);

namespace Nimbus\Auth;

use Arcano\Database\DatabaseInterface;
use Nimbus\Database\ArcanoDatabaseProvider;

/**
 * Rotating refresh tokens in the `refresh_tokens` table (see AuthTablesMigration). Each
 * `rotate()` spends the presented token and issues the next one of the same family; presenting
 * a spent token again revokes the family (reuse detection), which logs out a thief and the
 * victim alike. Only the SHA-256 of each 256-bit secret is stored.
 *
 * A family also has an absolute lifetime (`$familyLifetimeSeconds`, 30 days by default, null for
 * none): rotation never extends a token past it, so a stolen token that keeps being rotated
 * still dies and the user signs in again. The family identifier records when it started;
 * families issued before this limit existed keep rotating without it.
 */
final readonly class RefreshTokens implements RevokesCredentials
{
    private const string TABLE = 'refresh_tokens';

    private const array COLUMNS = ['id', 'family_id', 'subject', 'token_hash', 'abilities', 'expires_at', 'used_at', 'revoked_at'];

    private AuthDatabase $database;

    /**
     * @param int $ttlSeconds Lifetime of each refresh token.
     * @param (\Closure(): int)|null $clock
     */
    public function __construct(
        ArcanoDatabaseProvider|DatabaseInterface $database,
        private int $ttlSeconds = 2_592_000,
        private string $prefix = 'nbr_',
        ?string $connection = null,
        ?\Closure $clock = null,
        private ?int $familyLifetimeSeconds = 2_592_000,
    ) {
        if ($ttlSeconds < 60 || $ttlSeconds > 31_536_000 || preg_match('/^[A-Za-z0-9_-]{0,16}$/D', $prefix) !== 1) {
            throw new \InvalidArgumentException('Refresh token TTL must be one minute to one year, with a short prefix.');
        }
        if ($familyLifetimeSeconds !== null && ($familyLifetimeSeconds < 60 || $familyLifetimeSeconds > 31_536_000)) {
            throw new \InvalidArgumentException('The refresh token family lifetime must be one minute to one year.');
        }
        $this->database = new AuthDatabase($database, $connection, $clock);
    }

    /**
     * Starts a new family, e.g. at login.
     *
     * @param list<string> $abilities
     */
    public function issue(string $subject, array $abilities = ['*']): NewRefreshToken
    {
        if ($subject === '' || strlen($subject) > 191) {
            throw new \InvalidArgumentException('Refresh token subject must have 1 to 191 characters.');
        }
        // `t` + the start time in hex + 48 random bits: 23 characters, like the 24-character column.
        $family = sprintf('t%010x', $this->database->now()) . bin2hex(random_bytes(6));
        return $this->insert($this->database->db(), $family, $subject, Abilities::normalize($abilities));
    }

    /**
     * Spends the token and returns its successor; null when the token is unknown, expired or revoked.
     *
     * @throws RefreshTokenReusedException When the token was already spent.
     */
    public function rotate(#[\SensitiveParameter] string $plainTextToken): ?NewRefreshToken
    {
        $row = $this->lookup($plainTextToken);
        if ($row === null || $row['revoked_at'] !== null) {
            return null;
        }
        if ($row['used_at'] !== null) {
            $this->reused($row);
        }
        $now = $this->database->now();
        if ($row['expires_at'] <= $now || $this->familyEndsAt($row['family_id']) <= $now) {
            return null;
        }
        $db = $this->database->db();
        $next = $db->transaction(function () use ($db, $row, $now): ?NewRefreshToken {
            $spent = $db->update(self::TABLE, ['used_at' => $now], [
                'id' => $row['id'],
                'used_at' => null,
                'revoked_at' => null,
            ])->run();
            return $spent === 1 ? $this->insert($db, $row['family_id'], $row['subject'], $row['abilities']) : null;
        });
        if (!$next instanceof NewRefreshToken) {
            // Another request spent it first: the same token was presented twice.
            $this->reused($row);
        }
        return $next;
    }

    /** Revokes the token's family (logout). False for an unknown token. */
    public function revoke(#[\SensitiveParameter] string $plainTextToken): bool
    {
        $row = $this->lookup($plainTextToken);
        if ($row === null) {
            return false;
        }
        $this->revokeFamily($row['family_id']);
        return true;
    }

    public function revokeFamily(string $familyId): int
    {
        return $this->database->db()->update(self::TABLE, ['revoked_at' => $this->database->now()], [
            'family_id' => $familyId,
            'revoked_at' => null,
        ])->run();
    }

    /** Revokes every refresh token of the subject (logout everywhere). */
    public function revokeAll(string $subject): int
    {
        return $this->database->db()->update(self::TABLE, ['revoked_at' => $this->database->now()], [
            'subject' => $subject,
            'revoked_at' => null,
        ])->run();
    }

    /** Deletes tokens expired for longer than the grace period, kept until then for reuse detection. */
    public function prune(int $graceSeconds = 86_400): int
    {
        return $this->database->db()->delete(self::TABLE)
            ->where('expires_at', '<=', $this->database->now() - max(0, $graceSeconds))
            ->run();
    }

    /**
     * @return array{id: string, family_id: string, subject: string, abilities: list<string>, expires_at: int, used_at: ?int, revoked_at: ?int}|null
     */
    private function lookup(#[\SensitiveParameter] string $plainTextToken): ?array
    {
        $parsed = AuthDatabase::parse($plainTextToken, $this->prefix);
        if ($parsed === null) {
            return null;
        }
        $row = $this->database->first(self::TABLE, self::COLUMNS, ['id' => $parsed[0]]);
        if ($row === null || !hash_equals(AuthDatabase::string($row['token_hash'] ?? ''), AuthDatabase::hash($parsed[1]))) {
            return null;
        }
        return [
            'id' => AuthDatabase::string($row['id'] ?? ''),
            'family_id' => AuthDatabase::string($row['family_id'] ?? ''),
            'subject' => AuthDatabase::string($row['subject'] ?? ''),
            'abilities' => AuthDatabase::decodeAbilities($row['abilities'] ?? null),
            'expires_at' => AuthDatabase::int($row['expires_at'] ?? 0),
            'used_at' => AuthDatabase::nullableInt($row['used_at'] ?? null),
            'revoked_at' => AuthDatabase::nullableInt($row['revoked_at'] ?? null),
        ];
    }

    /** When the family stops rotating: its start plus the family lifetime (PHP_INT_MAX without one). */
    private function familyEndsAt(string $familyId): int
    {
        if ($this->familyLifetimeSeconds === null || preg_match('/^t([0-9a-f]{10})[0-9a-f]{12}$/D', $familyId, $match) !== 1) {
            return PHP_INT_MAX;
        }
        return (int) hexdec($match[1]) + $this->familyLifetimeSeconds;
    }

    /**
     * @param array{family_id: string, subject: string} $row
     * @return never
     */
    private function reused(array $row): never
    {
        $this->revokeFamily($row['family_id']);
        throw new RefreshTokenReusedException($row['family_id'], $row['subject']);
    }

    /** @param list<string> $abilities */
    private function insert(DatabaseInterface $db, string $familyId, string $subject, array $abilities): NewRefreshToken
    {
        $id = AuthDatabase::identifier();
        $secret = AuthDatabase::secret();
        $now = $this->database->now();
        $expiresAt = min($now + $this->ttlSeconds, $this->familyEndsAt($familyId));
        $db->insert(self::TABLE)->values([
            'id' => $id,
            'family_id' => $familyId,
            'subject' => $subject,
            'token_hash' => AuthDatabase::hash($secret),
            'abilities' => AuthDatabase::encodeAbilities($abilities),
            'expires_at' => $expiresAt,
            'used_at' => null,
            'revoked_at' => null,
            'created_at' => $now,
        ])->run();
        return new NewRefreshToken(
            "{$this->prefix}{$id}.{$secret}",
            new RefreshToken($id, $familyId, $subject, $abilities, $expiresAt),
        );
    }
}
