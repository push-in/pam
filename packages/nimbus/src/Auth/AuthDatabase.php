<?php

declare(strict_types=1);

namespace Nimbus\Auth;

use Arcano\Database\DatabaseInterface;
use Nimbus\Database\ArcanoDatabaseProvider;

/**
 * Database access, clock and secret helpers of the Arcano-backed auth stores.
 *
 * @internal
 */
final readonly class AuthDatabase
{
    /** @var \Closure(): int */
    private \Closure $clock;

    /** @param (\Closure(): int)|null $clock */
    public function __construct(
        private ArcanoDatabaseProvider|DatabaseInterface $database,
        private ?string $connection = null,
        ?\Closure $clock = null,
    ) {
        $this->clock = $clock ?? time(...);
    }

    public function db(): DatabaseInterface
    {
        return $this->database instanceof ArcanoDatabaseProvider
            ? $this->database->database($this->connection)
            : $this->database;
    }

    public function now(): int
    {
        return ($this->clock)();
    }

    /**
     * First row matching the conditions.
     *
     * @param list<string> $columns
     * @param array<string, string|int> $where
     * @return array<array-key, mixed>|null
     */
    public function first(string $table, array $columns, array $where): ?array
    {
        $query = $this->db()->select($columns)->from($table);
        foreach ($where as $column => $value) {
            $query->where($column, $value);
        }
        $row = $query->limit(1)->fetchAll()[0] ?? null;
        return is_array($row) ? $row : null;
    }

    /** A random identifier of 24 hex characters. */
    public static function identifier(): string
    {
        return bin2hex(random_bytes(12));
    }

    /** A random secret with 256 bits of entropy. */
    public static function secret(): string
    {
        return bin2hex(random_bytes(32));
    }

    public static function hash(#[\SensitiveParameter] string $secret): string
    {
        return hash('sha256', $secret);
    }

    /**
     * Splits `{prefix}{id}.{secret}` into the identifier and secret.
     *
     * @return array{string, string}|null
     */
    public static function parse(#[\SensitiveParameter] string $token, string $prefix = ''): ?array
    {
        if (strlen($token) > 256 || ($prefix !== '' && !str_starts_with($token, $prefix))) {
            return null;
        }
        $body = substr($token, strlen($prefix));
        if (preg_match('/^([a-f0-9]{24})\.([a-f0-9]{64})$/D', $body, $matches) !== 1) {
            return null;
        }
        return [$matches[1], $matches[2]];
    }

    /** @return list<string> */
    public static function decodeAbilities(mixed $value): array
    {
        if (!is_string($value) || $value === '') {
            return [];
        }
        try {
            $decoded = json_decode($value, true, 4, JSON_THROW_ON_ERROR);
        } catch (\JsonException) {
            return [];
        }
        $abilities = [];
        foreach (is_array($decoded) ? $decoded : [] as $ability) {
            if (is_string($ability) && $ability !== '') {
                $abilities[] = $ability;
            }
        }
        return $abilities;
    }

    /** @param list<string> $abilities */
    public static function encodeAbilities(array $abilities): string
    {
        return json_encode($abilities, JSON_THROW_ON_ERROR | JSON_UNESCAPED_SLASHES);
    }

    public static function int(mixed $value): int
    {
        return is_numeric($value) ? (int) $value : 0;
    }

    public static function nullableInt(mixed $value): ?int
    {
        return $value === null ? null : self::int($value);
    }

    public static function string(mixed $value): string
    {
        return is_scalar($value) ? (string) $value : '';
    }
}
