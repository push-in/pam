<?php

declare(strict_types=1);

namespace Nimbus\Database\Pgsql;

use Arcano\Database\Driver\PDOInterface;
use Arcano\Database\Driver\PDOStatementInterface;

/**
 * A PDO-shaped Postgres session on the PAM runtime's native client (pam_pg_*).
 * Errors are PDOExceptions carrying the server SQLSTATE, like pdo_pgsql's.
 *
 * @internal Created by NativePostgresDriver.
 */
final class NativePgConnection implements PDOInterface
{
    private const int SIMPLE = 1;

    private const int NUM = 2;

    private int $session;

    /** @var array<int, mixed> */
    private array $attributes = [];

    /**
     * @param array<string, string> $options libpq keywords (host, port, dbname, user, password, sslmode, ...)
     * @param array<int, mixed> $attributes PDO attributes; ATTR_DEFAULT_FETCH_MODE applies to statements
     */
    public function __construct(array $options, array $attributes = [])
    {
        $this->session = pam_pg_connect($options);
        $this->attributes = $attributes;
    }

    public function __destruct()
    {
        pam_pg_close($this->session);
    }

    /** False once a command was interrupted or the process forked; the driver must reconnect. */
    public function healthy(): bool
    {
        return pam_pg_status($this->session) !== -1;
    }

    /** @param array<mixed> $options */
    public function prepare(string $query, array $options = []): PDOStatementInterface
    {
        $statement = new NativePgStatement($this->session, $query);
        $mode = $this->attributes[\PDO::ATTR_DEFAULT_FETCH_MODE] ?? null;
        if (is_int($mode)) {
            $statement->setFetchMode($mode);
        }
        return $statement;
    }

    public function beginTransaction(): bool
    {
        pam_pg_transaction($this->session, 0);
        return true;
    }

    public function commit(): bool
    {
        pam_pg_transaction($this->session, 1);
        return true;
    }

    public function rollBack(): bool
    {
        pam_pg_transaction($this->session, 2);
        return true;
    }

    public function inTransaction(): bool
    {
        return pam_pg_status($this->session) > 0;
    }

    public function exec(string $statement): int
    {
        return pam_pg_execute($this->session, $statement, [], self::SIMPLE);
    }

    public function query(mixed $statement, mixed $mode = \PDO::ATTR_DEFAULT_FETCH_MODE, mixed ...$fetch_mode_args): PDOStatementInterface
    {
        if (!is_string($statement)) {
            throw new \InvalidArgumentException('The query must be a string.');
        }
        $prepared = $this->prepare($statement);
        if (is_int($mode) && $mode !== \PDO::ATTR_DEFAULT_FETCH_MODE) {
            $prepared->setFetchMode($mode, ...$fetch_mode_args);
        }
        $prepared->execute();
        return $prepared;
    }

    public function lastInsertId(?string $name = null): string|false
    {
        $result = $name === null
            ? pam_pg_execute($this->session, 'SELECT lastval()', [], self::NUM)
            : pam_pg_execute($this->session, 'SELECT currval(?)', [$name], self::NUM);
        $value = $result[2][0][0] ?? null;
        return is_int($value) || is_string($value) ? (string) $value : false;
    }

    public function setAttribute(int $attribute, mixed $value): bool
    {
        $this->attributes[$attribute] = $value;
        return true;
    }

    public function getAttribute(int $attribute): mixed
    {
        return match ($attribute) {
            \PDO::ATTR_DRIVER_NAME => 'pgsql',
            \PDO::ATTR_SERVER_VERSION => pam_pg_parameter($this->session, 'server_version'),
            \PDO::ATTR_CLIENT_VERSION => 'pam-pg',
            \PDO::ATTR_ERRMODE => \PDO::ERRMODE_EXCEPTION,
            default => $this->attributes[$attribute] ?? null,
        };
    }

    public function errorCode(): ?string
    {
        return null;
    }

    /** @return array{string, null, null} */
    public function errorInfo(): array
    {
        return ['00000', null, null];
    }

    /** A literal that is correct whatever standard_conforming_strings says (libpq's PQescapeLiteral). */
    public function quote(string $string, int $type = \PDO::PARAM_STR): string|false
    {
        if (str_contains($string, "\0")) {
            return false;
        }
        if (!str_contains($string, '\\')) {
            return "'" . str_replace("'", "''", $string) . "'";
        }
        return " E'" . str_replace(['\\', "'"], ['\\\\', "''"], $string) . "'";
    }
}
