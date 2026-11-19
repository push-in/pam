<?php

declare(strict_types=1);

namespace Nimbus\Database\Pgsql;

use Arcano\Database\Driver\PDOInterface;
use Arcano\Database\Driver\PDOStatementInterface;
use PgSql\Connection;
use PgSql\Result;

/**
 * A non-blocking Postgres session over ext-pgsql. Inside a PAM request Fiber every
 * wait suspends on the libpq socket, so one worker serves other requests while the
 * database works; elsewhere it waits with stream_select().
 *
 * Statements are prepared on the server once per session and executed by name.
 *
 * @internal Created by AsyncPostgresDriver.
 */
final class PgsqlConnection implements PDOInterface
{
    private Connection $connection;

    /** @var resource */
    private $socket;

    private bool $transaction = false;

    /** @var array<string, string> SQL => server-side statement name */
    private array $prepared = [];

    /** @var array<int, mixed> */
    private array $attributes = [];

    public function __construct(string $conninfo, private readonly float $timeout = 30.0)
    {
        $deadline = microtime(true) + $timeout;
        $connection = pg_connect($conninfo, PGSQL_CONNECT_ASYNC | PGSQL_CONNECT_FORCE_NEW);
        if (!$connection instanceof Connection) {
            throw PgsqlException::from('08001', 'Could not start the Postgres connection.');
        }
        $this->connection = $connection;
        $socket = pg_socket($connection);
        if (!is_resource($socket)) {
            throw PgsqlException::from('08001', 'Postgres connection has no socket.');
        }
        $this->socket = $socket;
        while (true) {
            $state = pg_connect_poll($connection);
            match ($state) {
                PGSQL_POLLING_OK => null,
                PGSQL_POLLING_READING => $this->wait(false, $deadline),
                PGSQL_POLLING_WRITING => $this->wait(true, $deadline),
                default => throw PgsqlException::from('08001', trim((string) pg_last_error($connection)) ?: 'Postgres connection failed.'),
            };
            if ($state === PGSQL_POLLING_OK) {
                break;
            }
        }
        // libpq replaces the socket while it negotiates; the final one is the session's.
        $socket = pg_socket($connection);
        if (is_resource($socket)) {
            $this->socket = $socket;
        }
    }

    public function __destruct()
    {
        pg_close($this->connection);
    }

    /**
     * Runs a prepared statement and returns its finished result.
     *
     * @param list<string|null> $parameters
     */
    public function run(string $sql, array $parameters): Result
    {
        if ($parameters === [] && !isset($this->prepared[$sql])) {
            return $this->send(static fn (Connection $c): bool => pg_send_query($c, $sql) !== false);
        }
        $name = $this->prepared[$sql] ?? null;
        if ($name === null) {
            $name = 'n' . count($this->prepared);
            $this->send(static fn (Connection $c): bool => pg_send_prepare($c, $name, self::positional($sql)) !== false);
            $this->prepared[$sql] = $name;
        }
        return $this->send(static fn (Connection $c): bool => pg_send_execute($c, $name, $parameters) !== false);
    }

    /** @param array<mixed> $options */
    public function prepare(string $query, array $options = []): PDOStatementInterface
    {
        return new PgsqlStatement($this, $query);
    }

    public function beginTransaction(): bool
    {
        $this->run('BEGIN', []);
        return $this->transaction = true;
    }

    public function commit(): bool
    {
        $this->run('COMMIT', []);
        $this->transaction = false;
        return true;
    }

    public function rollBack(): bool
    {
        $this->run('ROLLBACK', []);
        $this->transaction = false;
        return true;
    }

    public function inTransaction(): bool
    {
        return $this->transaction;
    }

    public function exec(string $statement): int
    {
        return pg_affected_rows($this->run($statement, []));
    }

    public function query(mixed $statement, mixed $mode = \PDO::ATTR_DEFAULT_FETCH_MODE, mixed ...$fetch_mode_args): PDOStatementInterface
    {
        if (!is_string($statement)) {
            throw new \InvalidArgumentException('The query must be a string.');
        }
        $prepared = new PgsqlStatement($this, $statement);
        $prepared->execute();
        return $prepared;
    }

    public function lastInsertId(?string $name = null): string|false
    {
        $sql = $name === null ? 'SELECT lastval()' : 'SELECT currval(' . pg_escape_literal($this->connection, $name) . ')';
        $value = pg_fetch_result($this->run($sql, []), 0, 0);
        return is_string($value) ? $value : false;
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
            \PDO::ATTR_SERVER_VERSION => pg_version($this->connection)['server'] ?? null,
            \PDO::ATTR_CLIENT_VERSION => pg_version($this->connection)['client'] ?? null,
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

    public function quote(string $string, int $type = \PDO::PARAM_STR): string|false
    {
        return pg_escape_literal($this->connection, $string);
    }

    /** @param \Closure(Connection): bool $send */
    private function send(\Closure $send): Result
    {
        $deadline = microtime(true) + $this->timeout;
        if (!$send($this->connection)) {
            throw $this->connectionError();
        }
        // Large statements may not leave in one write: 0 means more is pending.
        while (($flushed = pg_flush($this->connection)) !== true) {
            if ($flushed === false) {
                throw $this->connectionError();
            }
            $this->wait(true, $deadline);
        }
        $result = null;
        while (true) {
            while (pg_connection_busy($this->connection)) {
                $this->wait(false, $deadline);
                if (!pg_consume_input($this->connection)) {
                    throw $this->connectionError();
                }
            }
            $next = pg_get_result($this->connection);
            if ($next === false) {
                break;
            }
            $result = $next;
        }
        if (!$result instanceof Result) {
            throw $this->connectionError();
        }
        $status = pg_result_status($result);
        if ($status === PGSQL_FATAL_ERROR || $status === PGSQL_BAD_RESPONSE || $status === PGSQL_NONFATAL_ERROR) {
            $state = pg_result_error_field($result, PGSQL_DIAG_SQLSTATE);
            $message = trim((string) pg_result_error($result));
            throw PgsqlException::from(is_string($state) && $state !== '' ? $state : 'HY000', $message);
        }
        return $result;
    }

    private function wait(bool $write, float $deadline): void
    {
        $remaining = $deadline - microtime(true);
        if ($remaining <= 0) {
            throw PgsqlException::from('57014', 'Postgres did not answer before the timeout.');
        }
        if (function_exists('Pam\\Async\\readable') && function_exists('Pam\\Async\\writable')) {
            $write
                ? \Pam\Async\writable($this->socket, $remaining)
                : \Pam\Async\readable($this->socket, $remaining);
            return;
        }
        $read = $write ? [] : [$this->socket];
        $writable = $write ? [$this->socket] : [];
        $except = [];
        $seconds = (int) $remaining;
        if (@stream_select($read, $writable, $except, $seconds, (int) (($remaining - $seconds) * 1_000_000)) === 0) {
            throw PgsqlException::from('57014', 'Postgres did not answer before the timeout.');
        }
    }

    private function connectionError(): PgsqlException
    {
        $message = trim((string) pg_last_error($this->connection));
        return PgsqlException::from('08006', $message !== '' ? $message : 'The Postgres connection was lost.');
    }

    /** Rewrites `?` placeholders outside quotes and comments into `$1, $2, ...`. */
    private static function positional(string $sql): string
    {
        if (!str_contains($sql, '?')) {
            return $sql;
        }
        $out = '';
        $index = 0;
        $length = strlen($sql);
        for ($i = 0; $i < $length; ++$i) {
            $char = $sql[$i];
            if ($char === "'" || $char === '"') {
                // A doubled quote is an escaped quote inside the literal.
                $end = $i + 1;
                while ($end < $length) {
                    if ($sql[$end] === $char) {
                        if (($sql[$end + 1] ?? '') !== $char) {
                            break;
                        }
                        ++$end;
                    }
                    ++$end;
                }
                $out .= substr($sql, $i, $end - $i + 1);
                $i = $end;
                continue;
            }
            if ($char === '-' && ($sql[$i + 1] ?? '') === '-') {
                $end = strpos($sql, "\n", $i);
                $end = $end === false ? $length : $end;
                $out .= substr($sql, $i, $end - $i);
                $i = $end - 1;
                continue;
            }
            if ($char === '?' && ($sql[$i + 1] ?? '') !== '|' && ($sql[$i + 1] ?? '') !== '&' && ($sql[$i + 1] ?? '') !== '?') {
                $out .= '$' . ++$index;
                continue;
            }
            $out .= $char;
        }
        return $out;
    }
}
