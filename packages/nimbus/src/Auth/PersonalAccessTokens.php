<?php

declare(strict_types=1);

namespace Nimbus\Auth;

use Arcano\Database\DatabaseInterface;
use Nimbus\Database\ArcanoDatabaseProvider;

/**
 * Long-lived API tokens in the `personal_access_tokens` table (see AuthTablesMigration).
 * A token reads `{prefix}{id}.{secret}`; only the SHA-256 of the 256-bit secret is stored
 * and it is compared in constant time.
 */
final readonly class PersonalAccessTokens
{
    private const string TABLE = 'personal_access_tokens';

    private const array COLUMNS = ['id', 'subject', 'name', 'token_hash', 'abilities', 'last_used_at', 'expires_at', 'created_at'];

    private AuthDatabase $database;

    /**
     * @param string $prefix Marks the tokens for secret scanners, e.g. `nbp_`.
     * @param int $lastUsedResolution `last_used_at` is written at most once per this many seconds.
     * @param (\Closure(): int)|null $clock
     */
    public function __construct(
        ArcanoDatabaseProvider|DatabaseInterface $database,
        private string $prefix = 'nbp_',
        private int $lastUsedResolution = 60,
        ?string $connection = null,
        ?\Closure $clock = null,
    ) {
        if (preg_match('/^[A-Za-z0-9_-]{0,16}$/D', $prefix) !== 1 || $lastUsedResolution < 0) {
            throw new \InvalidArgumentException('The token prefix or last-used resolution is invalid.');
        }
        $this->database = new AuthDatabase($database, $connection, $clock);
    }

    /** @param list<string> $abilities */
    public function create(string $subject, string $name, array $abilities = ['*'], ?int $expiresAt = null): NewPersonalAccessToken
    {
        if ($subject === '' || strlen($subject) > 191 || $name === '' || strlen($name) > 191) {
            throw new \InvalidArgumentException('Token subject and name must have 1 to 191 characters.');
        }
        $abilities = Abilities::normalize($abilities);
        $now = $this->database->now();
        if ($expiresAt !== null && $expiresAt <= $now) {
            throw new \InvalidArgumentException('A token cannot expire in the past.');
        }
        $id = AuthDatabase::identifier();
        $secret = AuthDatabase::secret();
        $this->database->db()->insert(self::TABLE)->values([
            'id' => $id,
            'subject' => $subject,
            'name' => $name,
            'token_hash' => AuthDatabase::hash($secret),
            'abilities' => AuthDatabase::encodeAbilities($abilities),
            'last_used_at' => null,
            'expires_at' => $expiresAt,
            'created_at' => $now,
        ])->run();
        return new NewPersonalAccessToken(
            "{$this->prefix}{$id}.{$secret}",
            new PersonalAccessToken($id, $subject, $name, $abilities, null, $expiresAt, $now),
        );
    }

    /** The valid, unexpired token for a plain token, or null. */
    public function find(#[\SensitiveParameter] string $plainTextToken): ?PersonalAccessToken
    {
        $parsed = AuthDatabase::parse($plainTextToken, $this->prefix);
        if ($parsed === null) {
            return null;
        }
        $row = $this->database->first(self::TABLE, self::COLUMNS, ['id' => $parsed[0]]);
        if ($row === null || !hash_equals(AuthDatabase::string($row['token_hash'] ?? ''), AuthDatabase::hash($parsed[1]))) {
            return null;
        }
        $token = self::token($row);
        return $token->expired($this->database->now()) ? null : $token;
    }

    /** Finds the token and records its use, at most once per `lastUsedResolution` seconds. */
    public function use(#[\SensitiveParameter] string $plainTextToken): ?PersonalAccessToken
    {
        $token = $this->find($plainTextToken);
        if ($token === null) {
            return null;
        }
        $now = $this->database->now();
        if ($token->lastUsedAt === null || $now - $token->lastUsedAt >= $this->lastUsedResolution) {
            $this->database->db()->update(self::TABLE, ['last_used_at' => $now], ['id' => $token->id])->run();
            return new PersonalAccessToken(
                $token->id,
                $token->subject,
                $token->name,
                $token->abilities,
                $now,
                $token->expiresAt,
                $token->createdAt,
            );
        }
        return $token;
    }

    /** @return list<PersonalAccessToken> The subject's tokens, newest first. */
    public function tokensFor(string $subject): array
    {
        $rows = $this->database->db()->select(self::COLUMNS)->from(self::TABLE)
            ->where('subject', $subject)
            ->orderBy('created_at', 'DESC')
            ->orderBy('id', 'DESC')
            ->limit(1_000)
            ->fetchAll();
        $tokens = [];
        foreach ($rows as $row) {
            if (is_array($row)) {
                $tokens[] = self::token($row);
            }
        }
        return $tokens;
    }

    /** Revokes one token; pass the subject to make sure it belongs to that user. */
    public function revoke(string $id, ?string $subject = null): bool
    {
        $where = ['id' => $id];
        if ($subject !== null) {
            $where['subject'] = $subject;
        }
        return $this->database->db()->delete(self::TABLE, $where)->run() === 1;
    }

    /** Revokes every token of the subject and returns how many there were. */
    public function revokeAll(string $subject): int
    {
        return $this->database->db()->delete(self::TABLE, ['subject' => $subject])->run();
    }

    /** Deletes expired tokens. */
    public function prune(): int
    {
        return $this->database->db()->delete(self::TABLE)
            ->where('expires_at', '<=', $this->database->now())
            ->run();
    }

    /** @param array<array-key, mixed> $row */
    private static function token(array $row): PersonalAccessToken
    {
        return new PersonalAccessToken(
            AuthDatabase::string($row['id'] ?? ''),
            AuthDatabase::string($row['subject'] ?? ''),
            AuthDatabase::string($row['name'] ?? ''),
            AuthDatabase::decodeAbilities($row['abilities'] ?? null),
            AuthDatabase::nullableInt($row['last_used_at'] ?? null),
            AuthDatabase::nullableInt($row['expires_at'] ?? null),
            AuthDatabase::int($row['created_at'] ?? 0),
        );
    }
}
